use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::{Arc, RwLock},
    time::Duration,
};

use log::warn;

use crate::{ComponentKind, DiffMask, GlobalEntity, GlobalWorldManagerType};

use crate::world::entity_index::{EntityIndex, KeyGenerator32};
use crate::world::update::global_diff_handler::GlobalDiffHandler;
use crate::world::update::mut_channel::{DirtyNotifier, DirtySet, MutReceiver};

/// EntityIndex recycle timeout — long enough to cover packet drop / RTT
/// retries that may still reference an entity_idx briefly after dereg.
const ENTITY_INDEX_RECYCLE_TIMEOUT: Duration = Duration::from_secs(2);

// Diagnostic counters for the perf-upgrade project. These measure how much
// work `dirty_receiver_candidates` does per invocation. On idle ticks today
// the scan is O(receivers), which multiplied by users is the O(U·N) cost the
// matrix shows. After Phase 3 lands a dirty-push model, `receivers_visited`
// per idle tick should drop to zero. Enabled via `bench_instrumentation`.
#[cfg(feature = "bench_instrumentation")]
pub mod dirty_scan_counters {
    use std::sync::atomic::{AtomicU64, Ordering};
    pub static SCAN_CALLS: AtomicU64 = AtomicU64::new(0);
    pub static RECEIVERS_VISITED: AtomicU64 = AtomicU64::new(0);
    pub static DIRTY_RESULTS: AtomicU64 = AtomicU64::new(0);

    pub fn reset() {
        SCAN_CALLS.store(0, Ordering::Relaxed);
        RECEIVERS_VISITED.store(0, Ordering::Relaxed);
        DIRTY_RESULTS.store(0, Ordering::Relaxed);
    }
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
/// Phase 9.4 / Stage E (2026-04-25): per-user `EntityIndex` issuance lives
/// here — `entity_to_index` maps `GlobalEntity → EntityIndex`,
/// `index_to_entity` is the dense reverse table consulted at drain time.
/// `kinds_by_bit` records `kind_bit → ComponentKind` so the snapshot can
/// rebuild the legacy `HashMap<GlobalEntity, HashSet<ComponentKind>>`
/// shape consumers expect, without needing access to `ComponentKinds` on
/// the read path.
#[derive(Clone)]
pub struct UserDiffHandler {
    receivers: HashMap<(GlobalEntity, ComponentKind), MutReceiver>,
    global_diff_handler: Arc<RwLock<GlobalDiffHandler>>,
    /// Per-user dense indices for entities the handler is tracking.
    entity_to_index: HashMap<GlobalEntity, EntityIndex>,
    /// `index_to_entity[idx]` is `Some(entity)` while the entity is
    /// registered; `None` after deregistration (slot held until recycle).
    index_to_entity: Vec<Option<GlobalEntity>>,
    /// Refcount of registered components per entity_idx. When the count
    /// drops to zero we recycle the index.
    components_per_entity: HashMap<EntityIndex, u32>,
    /// EntityIndex allocator (recycling, u32 keyspace).
    key_gen: KeyGenerator32<EntityIndex>,
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
            entity_to_index: HashMap::new(),
            index_to_entity: Vec::new(),
            components_per_entity: HashMap::new(),
            key_gen: KeyGenerator32::new(ENTITY_INDEX_RECYCLE_TIMEOUT),
            kinds_by_bit: vec![None; kind_count as usize],
            dirty_set: Arc::new(DirtySet::new(kind_count)),
        }
    }

    fn allocate_entity_index(&mut self, entity: &GlobalEntity) -> EntityIndex {
        if let Some(&idx) = self.entity_to_index.get(entity) {
            return idx;
        }
        let idx = self.key_gen.generate();
        let slot = idx.0 as usize;
        if slot >= self.index_to_entity.len() {
            self.index_to_entity.resize(slot + 1, None);
        }
        self.index_to_entity[slot] = Some(*entity);
        self.entity_to_index.insert(*entity, idx);
        // B-strict: pre-grow the lock-free DirtyQueue's atomic bits
        // Vec to cover this slot before any mutation can reference it.
        // Cold-path RwLock write — never contended on the hot path
        // because mutations on this entity haven't started yet.
        self.dirty_set.ensure_capacity(slot);
        idx
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
        drop(global_handler);
        // GlobalDiffHandler should always be able to resolve kind_bit at this
        // point (component registration goes through the same ComponentKinds
        // that issued the receiver above). Bail with a no-op if not.
        let Some(kind_bit) = kind_bit else {
            warn!("UserDiffHandler: kind_bit unresolved for {:?}; notifier not attached", component_kind);
            return;
        };
        let entity_idx = self.allocate_entity_index(entity);

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
        *self.components_per_entity.entry(entity_idx).or_insert(0) += 1;
    }

    pub fn deregister_component(&mut self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        self.receivers.remove(&(*entity, *component_kind));

        let Some(&entity_idx) = self.entity_to_index.get(entity) else {
            return;
        };

        let Ok(global_handler) = self.global_diff_handler.as_ref().read() else {
            return;
        };
        if let Some(kind_bit) = global_handler.kind_bit(component_kind) {
            self.dirty_set.cancel(entity_idx, kind_bit);
        }
        drop(global_handler);

        // Refcount the entity_idx; recycle when no components remain.
        if let Some(count) = self.components_per_entity.get_mut(&entity_idx) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.components_per_entity.remove(&entity_idx);
                self.entity_to_index.remove(entity);
                if let Some(slot) = self.index_to_entity.get_mut(entity_idx.0 as usize) {
                    *slot = None;
                }
                self.key_gen.recycle_key(&entity_idx);
            }
        }
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
        // Lock-free drain. `drain()` atomically swap-zeroes every word
        // of each indexed entity and returns owned
        // `(EntityIndex, dirty_words)` pairs where `dirty_words` is
        // `Vec<u64>` of length `stride` (= `ceil(kind_count / 64)`).
        // Each set bit at word `w`, position `b` corresponds to
        // `kind_bit = w * 64 + b`.
        let drained: Vec<(EntityIndex, Vec<u64>)> = self.dirty_set.drain();

        // Re-mark the drained entries: this method is read-only by contract,
        // so the bits must persist for downstream callers / next call.
        // Each `push` is lock-free (atomic fetch_or under a read guard);
        // the cold-path indices mutex is acquired once per first-bit-set
        // per entity, which is the same number of pushes as before.
        for (idx, words) in &drained {
            for (word_idx, &word) in words.iter().enumerate() {
                let mut remaining = word;
                while remaining != 0 {
                    let bit = remaining.trailing_zeros() as u16;
                    let kind_bit = (word_idx as u16) * 64 + bit;
                    self.dirty_set.push(*idx, kind_bit);
                    remaining &= remaining - 1;
                }
            }
        }

        let mut result: HashMap<GlobalEntity, HashSet<ComponentKind>> =
            HashMap::with_capacity(drained.len());
        for (idx, words) in drained {
            let Some(Some(entity)) = self.index_to_entity.get(idx.0 as usize) else {
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
                result.insert(*entity, set);
            }
        }

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
