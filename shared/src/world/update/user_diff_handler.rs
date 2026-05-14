use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::{Arc, RwLock},
};

use log::warn;

use crate::{ComponentKind, DiffMask, GlobalEntity, GlobalWorldManagerType};

use crate::world::update::global_entity_index::GlobalEntityIndex;
use crate::world::update::global_diff_handler::GlobalDiffHandler;
use crate::world::update::mut_channel::{DirtyNotifier, DirtySet, MutReceiver};

// Diagnostic counters for the perf-upgrade project. These measure how much
// work `dirty_receiver_candidates` does per invocation. Phase 3 / C.4 landed
// the dirty-push model via `DirtySet::build_candidates`; `receivers_visited`
// on idle ticks (no component mutations) is now zero. Enabled via `bench_instrumentation`.
/// Diagnostic counters for the `dirty_receiver_candidates` scan.
#[cfg(feature = "bench_instrumentation")]
pub mod dirty_scan_counters {
    use std::sync::atomic::{AtomicU64, Ordering};
    #[doc(hidden)] pub static SCAN_CALLS: AtomicU64 = AtomicU64::new(0);
    #[doc(hidden)] pub static RECEIVERS_VISITED: AtomicU64 = AtomicU64::new(0);
    #[doc(hidden)] pub static DIRTY_RESULTS: AtomicU64 = AtomicU64::new(0);

    /// Resets all scan counters to zero.
    pub fn reset() {
        SCAN_CALLS.store(0, Ordering::Relaxed);
        RECEIVERS_VISITED.store(0, Ordering::Relaxed);
        DIRTY_RESULTS.store(0, Ordering::Relaxed);
    }
    /// Returns a snapshot of all scan counters as a tuple.
    pub fn snapshot() -> (u64, u64, u64) {
        (
            SCAN_CALLS.load(Ordering::Relaxed),
            RECEIVERS_VISITED.load(Ordering::Relaxed),
            DIRTY_RESULTS.load(Ordering::Relaxed),
        )
    }
}

/// Per-user diff handler.
///
/// Uses the global `GlobalEntityIndex` from `GlobalDiffHandler` for O(1) entity
/// lookup instead of per-user `LocalEntityIndex` management.
/// `kinds_by_bit` records `kind_bit → ComponentKind` so the snapshot can
/// rebuild the legacy `HashMap<GlobalEntity, HashSet<ComponentKind>>`
/// shape consumers expect, without needing access to `ComponentKinds` on
/// the read path.
#[derive(Clone)]
pub struct UserDiffHandler {
    receivers: HashMap<(GlobalEntity, ComponentKind), MutReceiver>,
    global_diff_handler: Arc<RwLock<GlobalDiffHandler>>,
    /// Reverse table for rebuilding `ComponentKind` from a `kind_bit`
    /// at snapshot time. Bit position == NetId per
    /// `ComponentKinds::add_component`. `None` at indices not yet
    /// registered. `Vec` (was fixed-size `[_; 64]`) since the
    /// 2026-05-05 unlimited-kind-count refactor — sized to the
    /// protocol's kind count at construction.
    kinds_by_bit: Vec<Option<ComponentKind>>,
    // Dirty-set bitset: per-user pure-CPU bookkeeping.
    // `MutReceiver::mutate` fires `notify_dirty` on clean→dirty
    // transitions; the resulting push is a Vec OR + (cold) push.
    dirty_set: Arc<DirtySet>,
}

impl UserDiffHandler {
    pub fn new(global_world_manager: &dyn GlobalWorldManagerType) -> Self {
        // Read the protocol's component-kind count under a brief read
        // guard. Used to size the per-user `DirtyQueue`'s stride and
        // the `kinds_by_bit` reverse-lookup table. The protocol is
        // already locked by the time any `UserDiffHandler` is
        // constructed (lock happens at server/client startup, before
        // the first connection), so `kind_count` is stable.
        let global_diff_handler = global_world_manager.diff_handler();
        let kind_count = global_diff_handler
            .read()
            .map(|h| h.kind_count())
            .unwrap_or(0);
        Self {
            receivers: HashMap::new(),
            global_diff_handler,
            kinds_by_bit: vec![None; kind_count as usize],
            dirty_set: Arc::new(DirtySet::new(kind_count)),
        }
    }

    // Component Registration
    pub fn register_component(
        &mut self,
        address: &Option<SocketAddr>,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) {
        let Ok(global_handler) = self.global_diff_handler.as_ref().read() else {
            panic!("Be sure you can get self.global_diff_handler before calling this!");
        };
        let Some(receiver) = global_handler.receiver(address, entity, component_kind) else {
            // Component not yet registered in GlobalDiffHandler - this can happen on the client
            // side when authority is granted before components are registered for diff tracking.
            // Skip registration for now; it will be registered when the component is actually
            // inserted or when it needs to be diffed.
            #[cfg(feature = "e2e_debug")]
            {
                warn!(
                    "UserDiffHandler: Component {:?} for {:?} not yet registered in GlobalDiffHandler, skipping registration",
                    component_kind, entity
                );
            }
            return;
        };

        let kind_bit = global_handler.kind_bit(component_kind);
        let entity_idx = global_handler.entity_to_global_idx(entity);
        drop(global_handler);
        // GlobalDiffHandler should always be able to resolve kind_bit at this
        // point (component registration goes through the same ComponentKinds
        // that issued the receiver above). Bail with a no-op if not.
        let Some(kind_bit) = kind_bit else {
            warn!("UserDiffHandler: kind_bit unresolved for {:?}; notifier not attached", component_kind);
            return;
        };
        let Some(entity_idx) = entity_idx else {
            #[cfg(feature = "e2e_debug")]
            warn!(
                "UserDiffHandler::register_component: entity {:?} not in global registry",
                entity
            );
            return;
        };
        self.dirty_set.ensure_capacity(entity_idx.as_usize());

        // Cache kind_bit → ComponentKind once for snapshot decode.
        // Defensive grow: if a kind was registered with the
        // GlobalDiffHandler AFTER this UserDiffHandler was constructed
        // (shouldn't happen post-protocol-lock, but tolerate it), the
        // Vec needs to grow.
        let bit_idx = kind_bit as usize;
        if bit_idx >= self.kinds_by_bit.len() {
            self.kinds_by_bit.resize(bit_idx + 1, None);
        }
        if self.kinds_by_bit[bit_idx].is_none() {
            self.kinds_by_bit[bit_idx] = Some(*component_kind);
        }

        receiver.attach_notifier(DirtyNotifier::new(
            entity_idx,
            kind_bit,
            Arc::downgrade(&self.dirty_set),
        ));
        self.receivers.insert((*entity, *component_kind), receiver);
    }

    pub fn deregister_component(&mut self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        self.receivers.remove(&(*entity, *component_kind));

        let Ok(global_handler) = self.global_diff_handler.as_ref().read() else {
            return;
        };
        let Some(entity_idx) = global_handler.entity_to_global_idx(entity) else {
            return;
        };
        if let Some(kind_bit) = global_handler.kind_bit(component_kind) {
            self.dirty_set.cancel(entity_idx, kind_bit);
        }
        drop(global_handler);
    }

    pub fn has_component(&self, entity: &GlobalEntity, component: &ComponentKind) -> bool {
        self.receivers.contains_key(&(*entity, *component))
    }

    // Diff masks
    pub fn diff_mask_snapshot(
        &self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) -> DiffMask {
        let Some(receiver) = self.receivers.get(&(*entity, *component_kind)) else {
            panic!("Should not call this unless we're sure there's a receiver");
        };
        receiver.mask_snapshot()
    }

    pub fn diff_mask_is_clear(
        &self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) -> bool {
        let Some(receiver) = self.receivers.get(&(*entity, *component_kind)) else {
            warn!(
                "diff_mask_is_clear(): Should not call this unless we're sure there's a receiver"
            );
            return true;
        };
        receiver.diff_mask_is_clear()
    }

    pub fn or_diff_mask(
        &mut self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
        other_mask: &DiffMask,
    ) {
        let Some(receiver) = self.receivers.get_mut(&(*entity, *component_kind)) else {
            panic!("Should not call this unless we're sure there's a receiver");
        };
        receiver.or_mask(other_mask);
    }

    pub fn clear_diff_mask(&mut self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        let Some(receiver) = self.receivers.get_mut(&(*entity, *component_kind)) else {
            panic!("Should not call this unless we're sure there's a receiver");
        };
        receiver.clear_mask();
    }

    #[cfg(feature = "test_utils")]
    pub fn receiver_count(&self) -> usize {
        self.receivers.len()
    }

    #[cfg(feature = "test_utils")]
    pub fn dirty_candidates_count(&self) -> usize {
        self.receivers.values().filter(|r| !r.diff_mask_is_clear()).count()
    }

    pub fn dirty_receiver_candidates(&self) -> HashMap<GlobalEntity, HashSet<ComponentKind>> {
        // Phase 3 / C.4 dirty-push model.
        //
        // `build_candidates()` reads dirty bits without zeroing them, and
        // refeeds entities that are still dirty so they appear next tick too.
        // Entities are removed from tracking only when `cancel()` clears their
        // bits — which happens in `clear_diff_mask()` → `record_update()` after
        // a component update is serialised into a packet.
        //
        // Entities that are not sent (bandwidth-deferred or out-of-scope) keep
        // their bits set and stay in the refeed list automatically — no O(U·N)
        // re-push loop needed.
        let candidates: Vec<(GlobalEntityIndex, Vec<u64>)> = self.dirty_set.build_candidates();

        let mut result: HashMap<GlobalEntity, HashSet<ComponentKind>> =
            HashMap::with_capacity(candidates.len());
        let Ok(global_handler) = self.global_diff_handler.read() else {
            return result;
        };
        for (idx, words) in candidates {
            let Some(entity) = global_handler.global_entity_at(idx) else {
                continue;
            };
            let mut set = HashSet::new();
            for (word_idx, word) in words.into_iter().enumerate() {
                let mut remaining = word;
                while remaining != 0 {
                    let bit = remaining.trailing_zeros() as usize;
                    let absolute_bit = word_idx * 64 + bit;
                    if let Some(Some(kind)) = self.kinds_by_bit.get(absolute_bit) {
                        set.insert(*kind);
                    }
                    remaining &= remaining - 1;
                }
            }
            if !set.is_empty() {
                result.insert(entity, set);
            }
        }
        drop(global_handler);

        #[cfg(feature = "bench_instrumentation")]
        {
            use std::sync::atomic::Ordering;
            dirty_scan_counters::SCAN_CALLS.fetch_add(1, Ordering::Relaxed);
            let visited: u64 = result.values().map(|s| s.len() as u64).sum();
            dirty_scan_counters::RECEIVERS_VISITED.fetch_add(visited, Ordering::Relaxed);
            dirty_scan_counters::DIRTY_RESULTS.fetch_add(visited, Ordering::Relaxed);
        }
        result
    }
}
