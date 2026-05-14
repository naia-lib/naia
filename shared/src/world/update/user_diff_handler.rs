use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::{Arc, RwLock, Weak},
};

use log::warn;

use crate::{ComponentKind, DiffMask, GlobalEntity, GlobalWorldManagerType};

use crate::world::update::global_dirty_bitset::GlobalDirtyBitset;
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
///
/// Hot-path methods take `(GlobalEntityIndex, u16)` directly to avoid the
/// (GlobalEntity, ComponentKind) → compact key resolution cost. The HashMap key
/// is `(GlobalEntityIndex, u16)` — 6 bytes vs the old ~17-byte tuple — for
/// better cache behavior and faster hashing on the hot path.
#[derive(Clone)]
pub struct UserDiffHandler {
    receivers: HashMap<(GlobalEntityIndex, u16), MutReceiver>,
    /// Reverse lookup for deregistration: (GlobalEntity, ComponentKind) → compact key.
    /// Populated at register_component; lets deregister_component find the key even after
    /// the entity is removed from GlobalDiffHandler.
    entity_kind_to_key: HashMap<(GlobalEntity, ComponentKind), (GlobalEntityIndex, u16)>,
    global_diff_handler: Arc<RwLock<GlobalDiffHandler>>,
    /// Reverse table for rebuilding `ComponentKind` from a `kind_bit`
    /// at snapshot time. Bit position == NetId per
    /// `ComponentKinds::add_component`. `None` at indices not yet
    /// registered. `Vec` (was fixed-size `[_; 64]`) since the
    /// 2026-05-05 unlimited-kind-count refactor — sized to the
    /// protocol's kind count at construction.
    kinds_by_bit: Vec<Option<ComponentKind>>,
    // Per-user dirty-set bitset for the CLIENT path — `None` on the server path.
    //
    // The server uses the GlobalDirtyBitset + ConnectionVisibilityBitset intersection
    // (Phase 9 three-phase loop) and never reads from this DirtySet. Keeping it `None`
    // on the server eliminates the wasted DirtySet push/cancel atomic operations that
    // would otherwise fire on every component mutation for every user.
    //
    // The client has no GlobalDirtyBitset, so it uses this DirtySet via
    // `dirty_receiver_candidates()` → `take_update_events()`.
    dirty_set: Option<Arc<DirtySet>>,
    // Server-global dirty bitset. `Weak` so it's a no-op on the client side
    // (where `global_dirty_bitset()` returns `None`).
    global_dirty: Weak<GlobalDirtyBitset>,
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
        let global_dirty_arc = global_world_manager.global_dirty_bitset();
        let global_dirty = global_dirty_arc
            .as_ref()
            .map(Arc::downgrade)
            .unwrap_or_else(Weak::new);
        // Server path: GlobalDirtyBitset is present — the three-phase Iris send loop
        // reads GlobalDirtyBitset directly, so per-user DirtySet is never consumed.
        // Skip allocating it to avoid wasted push/cancel atomic ops on every mutation.
        // Client path: no GlobalDirtyBitset — need DirtySet for dirty candidate tracking.
        let dirty_set = if global_dirty_arc.is_none() {
            Some(Arc::new(DirtySet::new(kind_count)))
        } else {
            None
        };
        Self {
            receivers: HashMap::new(),
            entity_kind_to_key: HashMap::new(),
            global_diff_handler,
            kinds_by_bit: vec![None; kind_count as usize],
            dirty_set,
            global_dirty,
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
        if let Some(dirty_set) = &self.dirty_set {
            dirty_set.ensure_capacity(entity_idx.as_usize());
        }

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

        // Server path: dirty_set is None — pass a dead Weak so DirtyNotifier's
        // set.upgrade() returns None and push/cancel are no-ops.
        let dirty_set_weak = self.dirty_set.as_ref().map(Arc::downgrade).unwrap_or_else(Weak::new);
        receiver.attach_notifier(DirtyNotifier::new(
            entity_idx,
            kind_bit,
            dirty_set_weak,
            self.global_dirty.clone(),
        ));
        self.entity_kind_to_key.insert((*entity, *component_kind), (entity_idx, kind_bit));
        self.receivers.insert((entity_idx, kind_bit), receiver);
    }

    pub fn deregister_component(&mut self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        let Some((entity_idx, kind_bit)) = self.entity_kind_to_key.remove(&(*entity, *component_kind)) else {
            // Never registered (or already deregistered) — nothing to clean up.
            return;
        };
        self.receivers.remove(&(entity_idx, kind_bit));

        // Only the client path has a DirtySet to cancel from.
        if let Some(dirty_set) = &self.dirty_set {
            dirty_set.cancel(entity_idx, kind_bit);
        }
    }

    pub fn has_component(&self, entity: &GlobalEntity, component: &ComponentKind) -> bool {
        self.entity_kind_to_key.contains_key(&(*entity, *component))
    }

    // Resolves (GlobalEntity, ComponentKind) → compact key (GlobalEntityIndex, u16).
    // Used by cold-path methods. Returns None if either lookup fails.
    fn compact_key(
        &self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) -> Option<(GlobalEntityIndex, u16)> {
        let guard = self.global_diff_handler.read().ok()?;
        let idx = guard.entity_to_global_idx(entity)?;
        let bit = guard.kind_bit(component_kind)?;
        Some((idx, bit))
    }

    // Diff masks — cold paths take (&GlobalEntity, &ComponentKind) and resolve internally.
    pub fn diff_mask_snapshot(
        &self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) -> DiffMask {
        let key = self.compact_key(entity, component_kind)
            .expect("Should not call this unless we're sure there's a receiver");
        let Some(receiver) = self.receivers.get(&key) else {
            panic!("Should not call this unless we're sure there's a receiver");
        };
        receiver.mask_snapshot()
    }

    pub fn diff_mask_is_clear(
        &self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) -> bool {
        let Some(key) = self.compact_key(entity, component_kind) else {
            warn!("diff_mask_is_clear(): Could not resolve compact key");
            return true;
        };
        let Some(receiver) = self.receivers.get(&key) else {
            warn!("diff_mask_is_clear(): Should not call this unless we're sure there's a receiver");
            return true;
        };
        receiver.diff_mask_is_clear()
    }

    /// Marks the receiver for `(entity, component_kind)` as delivered.
    /// Called by the delivery-confirmation path when a spawn/insert ACK arrives.
    pub fn mark_receiver_delivered(&self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        if let Some(key) = self.compact_key(entity, component_kind) {
            if let Some(receiver) = self.receivers.get(&key) {
                receiver.mark_delivered();
            }
        }
    }

    /// Cold-path combined check for `(GlobalEntity, ComponentKind)` — resolves compact key via RwLock.
    pub fn is_receiver_dirty_and_delivered(
        &self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) -> bool {
        let Some(key) = self.compact_key(entity, component_kind) else { return false; };
        self.receivers
            .get(&key)
            .map(|r| r.is_dirty_and_delivered())
            .unwrap_or(false)
    }

    /// Hot-path combined check for Phase 3: direct compact-key lookup, no RwLock.
    /// `entity_idx` and `kind_bit` are pre-resolved by the Phase 3 bitset scan.
    pub fn is_receiver_dirty_and_delivered_fast(
        &self,
        entity_idx: GlobalEntityIndex,
        kind_bit: u16,
    ) -> bool {
        self.receivers
            .get(&(entity_idx, kind_bit))
            .map(|r| r.is_dirty_and_delivered())
            .unwrap_or(false)
    }

    /// Hot-path diff mask check for Phase 3: direct compact-key lookup, no RwLock.
    pub fn diff_mask_is_clear_fast(&self, entity_idx: GlobalEntityIndex, kind_bit: u16) -> bool {
        self.receivers
            .get(&(entity_idx, kind_bit))
            .map(|r| r.diff_mask_is_clear())
            .unwrap_or(true)
    }

    /// Hot-path mask snapshot for write_update: direct compact-key lookup, no RwLock.
    /// Returns `None` if no receiver is registered for this (entity_idx, kind_bit).
    pub fn diff_mask_snapshot_fast(
        &self,
        entity_idx: GlobalEntityIndex,
        kind_bit: u16,
    ) -> Option<DiffMask> {
        self.receivers.get(&(entity_idx, kind_bit)).map(|r| r.mask_snapshot())
    }

    pub fn or_diff_mask(
        &mut self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
        other_mask: &DiffMask,
    ) {
        let key = self.compact_key(entity, component_kind)
            .expect("Should not call this unless we're sure there's a receiver");
        let Some(receiver) = self.receivers.get_mut(&key) else {
            panic!("Should not call this unless we're sure there's a receiver");
        };
        receiver.or_mask(other_mask);
    }

    pub fn clear_diff_mask(&mut self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        let key = self.compact_key(entity, component_kind)
            .expect("Should not call this unless we're sure there's a receiver");
        let Some(receiver) = self.receivers.get_mut(&key) else {
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

    /// Builds the dirty candidate set for this connection from the per-user DirtySet.
    /// CLIENT PATH ONLY — returns an empty map on the server, which uses the
    /// GlobalDirtyBitset + ConnectionVisibilityBitset three-phase loop instead.
    pub fn dirty_receiver_candidates(&self) -> HashMap<GlobalEntity, HashSet<ComponentKind>> {
        // Server path: no DirtySet allocated — the Iris three-phase loop drives candidate
        // selection from GlobalDirtyBitset directly. This path should never be called
        // on the server; return empty as a safe no-op.
        let Some(dirty_set) = &self.dirty_set else {
            return HashMap::new();
        };

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
        let candidates: Vec<(GlobalEntityIndex, Vec<u64>)> = dirty_set.build_candidates();

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
