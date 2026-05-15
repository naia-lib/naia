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
/// `receivers_dense` is a stride-indexed flat `Vec<Option<MutReceiver>>`.
/// Slot formula: `entity_idx.as_usize() * kind_count + kind_bit as usize`.
/// This gives O(1) array access in Phase 3 and `write_update` with no hashing.
///
/// `entity_kind_to_key` maps `(GlobalEntity, ComponentKind) → (GlobalEntityIndex, u16)`.
/// It is populated at registration time and used by cold-path methods, eliminating
/// any RwLock acquisition on `GlobalDiffHandler` for the per-connection diff paths.
///
/// `kinds_by_bit` records `kind_bit → ComponentKind` so `dirty_receiver_candidates`
/// can rebuild the `HashMap<GlobalEntity, HashSet<ComponentKind>>` shape that
/// callers expect, without needing access to `ComponentKinds` on the read path.
///
/// Hot-path methods take `(GlobalEntityIndex, u16)` directly — O(1) array access.
/// Cold-path methods take `(&GlobalEntity, &ComponentKind)` and resolve via `entity_kind_to_key`.
#[derive(Clone)]
pub struct UserDiffHandler {
    /// Stride-indexed flat receiver array. Slot = entity_idx * kind_count + kind_bit.
    /// `None` for unregistered (entity, component) pairs.
    receivers_dense: Vec<Option<MutReceiver>>,
    /// Number of component kinds. Fixed at construction (protocol is locked before
    /// any connection is established). Used as the stride for slot calculation.
    kind_count: usize,
    /// Reverse lookup: (GlobalEntity, ComponentKind) → (GlobalEntityIndex, kind_bit).
    /// Populated at `register_component`; removed at `deregister_component`.
    /// Used by cold-path methods and by `deregister_component` to avoid needing the
    /// GlobalDiffHandler RwLock after the entity may already have been freed.
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
            .map(|h| h.kind_count() as usize)
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
            Some(Arc::new(DirtySet::new(kind_count as u16)))
        } else {
            None
        };
        Self {
            receivers_dense: Vec::new(),
            kind_count,
            entity_kind_to_key: HashMap::new(),
            global_diff_handler,
            kinds_by_bit: vec![None; kind_count],
            dirty_set,
            global_dirty,
        }
    }

    // Returns the flat-array slot for (entity_idx, kind_bit).
    // Panics only if kind_count is zero — which cannot happen in a registered protocol.
    #[inline]
    fn slot(&self, entity_idx: GlobalEntityIndex, kind_bit: u16) -> usize {
        entity_idx.as_usize() * self.kind_count + kind_bit as usize
    }

    // Grows `receivers_dense` so that all slots for `entity_idx` exist.
    fn ensure_dense_capacity(&mut self, entity_idx: GlobalEntityIndex) {
        let needed = (entity_idx.as_usize() + 1) * self.kind_count;
        if needed > self.receivers_dense.len() {
            self.receivers_dense.resize_with(needed, || None);
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

        self.ensure_dense_capacity(entity_idx);
        let slot = self.slot(entity_idx, kind_bit);
        self.receivers_dense[slot] = Some(receiver);
        self.entity_kind_to_key.insert((*entity, *component_kind), (entity_idx, kind_bit));
    }

    pub fn deregister_component(&mut self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        let Some((entity_idx, kind_bit)) = self.entity_kind_to_key.remove(&(*entity, *component_kind)) else {
            // Never registered (or already deregistered) — nothing to clean up.
            return;
        };
        let slot = self.slot(entity_idx, kind_bit);
        if slot < self.receivers_dense.len() {
            self.receivers_dense[slot] = None;
        }

        // Only the client path has a DirtySet to cancel from.
        if let Some(dirty_set) = &self.dirty_set {
            dirty_set.cancel(entity_idx, kind_bit);
        }
    }

    pub fn has_component(&self, entity: &GlobalEntity, component: &ComponentKind) -> bool {
        self.entity_kind_to_key.contains_key(&(*entity, *component))
    }

    // Diff masks — cold paths resolve via entity_kind_to_key (no RwLock required).

    pub fn diff_mask_snapshot(
        &self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) -> DiffMask {
        let (entity_idx, kind_bit) = self.entity_kind_to_key
            .get(&(*entity, *component_kind))
            .copied()
            .expect("Should not call this unless we're sure there's a receiver");
        let slot = self.slot(entity_idx, kind_bit);
        let Some(Some(receiver)) = self.receivers_dense.get(slot) else {
            panic!("Should not call this unless we're sure there's a receiver");
        };
        receiver.mask_snapshot()
    }

    pub fn diff_mask_is_clear(
        &self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) -> bool {
        let Some((entity_idx, kind_bit)) = self.entity_kind_to_key.get(&(*entity, *component_kind)).copied() else {
            return true;
        };
        let slot = self.slot(entity_idx, kind_bit);
        match self.receivers_dense.get(slot) {
            Some(Some(r)) => r.diff_mask_is_clear(),
            _ => true,
        }
    }

    /// Marks the receiver for `(entity, component_kind)` as delivered.
    /// Called by the delivery-confirmation path when a spawn/insert ACK arrives.
    pub fn mark_receiver_delivered(&self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        let Some((entity_idx, kind_bit)) = self.entity_kind_to_key.get(&(*entity, *component_kind)).copied() else {
            return;
        };
        let slot = self.slot(entity_idx, kind_bit);
        if let Some(Some(receiver)) = self.receivers_dense.get(slot) {
            receiver.mark_delivered();
        }
    }

    /// Cold-path combined check — resolves via entity_kind_to_key.
    pub fn is_receiver_dirty_and_delivered(
        &self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) -> bool {
        let Some((entity_idx, kind_bit)) = self.entity_kind_to_key.get(&(*entity, *component_kind)).copied() else {
            return false;
        };
        let slot = self.slot(entity_idx, kind_bit);
        match self.receivers_dense.get(slot) {
            Some(Some(r)) => r.is_dirty_and_delivered(),
            _ => false,
        }
    }

    /// Hot-path combined check for Phase 3: O(1) array access, no hashing, no RwLock.
    /// `entity_idx` and `kind_bit` are pre-resolved by the Phase 3 bitset scan.
    pub fn is_receiver_dirty_and_delivered_fast(
        &self,
        entity_idx: GlobalEntityIndex,
        kind_bit: u16,
    ) -> bool {
        let slot = entity_idx.as_usize() * self.kind_count + kind_bit as usize;
        match self.receivers_dense.get(slot) {
            Some(Some(r)) => r.is_dirty_and_delivered(),
            _ => false,
        }
    }

    /// Hot-path diff mask check for Phase 3: O(1) array access, no hashing, no RwLock.
    pub fn diff_mask_is_clear_fast(&self, entity_idx: GlobalEntityIndex, kind_bit: u16) -> bool {
        let slot = entity_idx.as_usize() * self.kind_count + kind_bit as usize;
        match self.receivers_dense.get(slot) {
            Some(Some(r)) => r.diff_mask_is_clear(),
            _ => true,
        }
    }

    /// Hot-path mask snapshot for write_update: O(1) array access, no hashing, no RwLock.
    /// Returns `None` if no receiver is registered for this (entity_idx, kind_bit).
    pub fn diff_mask_snapshot_fast(
        &self,
        entity_idx: GlobalEntityIndex,
        kind_bit: u16,
    ) -> Option<DiffMask> {
        let slot = entity_idx.as_usize() * self.kind_count + kind_bit as usize;
        match self.receivers_dense.get(slot) {
            Some(Some(r)) => Some(r.mask_snapshot()),
            _ => None,
        }
    }

    pub fn or_diff_mask(
        &mut self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
        other_mask: &DiffMask,
    ) {
        let (entity_idx, kind_bit) = self.entity_kind_to_key
            .get(&(*entity, *component_kind))
            .copied()
            .expect("Should not call this unless we're sure there's a receiver");
        let slot = self.slot(entity_idx, kind_bit);
        let Some(Some(receiver)) = self.receivers_dense.get(slot) else {
            panic!("Should not call this unless we're sure there's a receiver");
        };
        receiver.or_mask(other_mask);
    }

    pub fn clear_diff_mask(&mut self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        let (entity_idx, kind_bit) = self.entity_kind_to_key
            .get(&(*entity, *component_kind))
            .copied()
            .expect("Should not call this unless we're sure there's a receiver");
        let slot = self.slot(entity_idx, kind_bit);
        let Some(Some(receiver)) = self.receivers_dense.get(slot) else {
            panic!("Should not call this unless we're sure there's a receiver");
        };
        receiver.clear_mask();
    }

    /// Hot-path clear: O(1) array access, no hashing, no RwLock.
    pub fn clear_diff_mask_fast(&mut self, entity_idx: GlobalEntityIndex, kind_bit: u16) {
        let slot = entity_idx.as_usize() * self.kind_count + kind_bit as usize;
        if let Some(Some(receiver)) = self.receivers_dense.get(slot) {
            receiver.clear_mask();
        }
    }

    #[cfg(feature = "test_utils")]
    pub fn receiver_count(&self) -> usize {
        self.receivers_dense.iter().filter(|s| s.is_some()).count()
    }

    #[cfg(feature = "test_utils")]
    pub fn dirty_candidates_count(&self) -> usize {
        self.receivers_dense.iter()
            .filter_map(|slot| slot.as_ref())
            .filter(|r| !r.diff_mask_is_clear())
            .count()
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

#[cfg(test)]
mod dense_receiver_tests {
    //! Pins the C.7.B invariant: stride-indexed flat Vec gives correct receiver
    //! retrieval across alloc/free/re-alloc sequences that exercise index recycling.
    //!
    //! These tests verify the slot arithmetic and array management directly,
    //! without wiring the full network stack.

    use crate::world::update::global_entity_index::GlobalEntityIndex;

    /// Slot arithmetic: entity_idx.as_usize() * kind_count + kind_bit must be injective.
    #[test]
    fn slot_arithmetic_is_injective() {
        let kind_count = 8usize;
        let mut seen = std::collections::HashSet::new();
        for entity_raw in 1u32..=32 {
            for kind_bit in 0u16..kind_count as u16 {
                let slot = entity_raw as usize * kind_count + kind_bit as usize;
                assert!(seen.insert(slot), "collision at entity={entity_raw} kind_bit={kind_bit}");
            }
        }
    }

    /// ensure_dense_capacity grows correctly for entity_idx = 1..=32.
    #[test]
    fn ensure_capacity_grows_monotonically() {
        let kind_count = 4usize;
        let mut vec: Vec<Option<u32>> = Vec::new();
        for entity_raw in 1u32..=32 {
            let entity_idx = GlobalEntityIndex(entity_raw);
            let needed = (entity_idx.as_usize() + 1) * kind_count;
            if needed > vec.len() {
                vec.resize_with(needed, || None);
            }
            // Every slot for this entity_idx must be in bounds.
            for kind_bit in 0..kind_count {
                let slot = entity_idx.as_usize() * kind_count + kind_bit;
                assert!(slot < vec.len(), "slot {slot} out of bounds after grow for entity={entity_raw}");
            }
        }
        // After 32 entities at stride 4: need 33 * 4 = 132 slots.
        assert_eq!(vec.len(), 33 * kind_count);
    }

    /// Slot reuse after free: entity A frees its slots, entity B gets the same
    /// GlobalEntityIndex.  The dense array must not retain A's slot value.
    #[test]
    fn freed_entity_slot_does_not_alias_new_entity() {
        let kind_count = 4usize;
        let mut vec: Vec<Option<u32>> = Vec::new();

        // Allocate entity A at index 3, kind_bit 2 → slot 14.
        let idx_a = GlobalEntityIndex(3);
        let needed = (idx_a.as_usize() + 1) * kind_count;
        vec.resize_with(needed, || None);
        let slot_a = idx_a.as_usize() * kind_count + 2;
        vec[slot_a] = Some(42u32); // sentinel value for A

        // Free A — clear all its slots.
        for kb in 0..kind_count {
            let s = idx_a.as_usize() * kind_count + kb;
            if s < vec.len() { vec[s] = None; }
        }

        // B gets the recycled index 3.
        let idx_b = GlobalEntityIndex(3);
        let slot_b = idx_b.as_usize() * kind_count + 2;
        assert!(vec[slot_b].is_none(), "slot must be None after free, not alias A's value");

        // Register B at the same slot.
        vec[slot_b] = Some(99u32);
        assert_eq!(vec[slot_b], Some(99u32));
    }
}
