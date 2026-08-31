use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::{Arc, RwLock, Weak},
};

use log::warn;

use crate::{ComponentKind, DiffMask, GlobalEntity, GlobalWorldManagerType};

use crate::world::update::global_diff_handler::GlobalDiffHandler;
use crate::world::update::global_dirty_bitset::GlobalDirtyBitset;
use crate::world::update::global_entity_index::GlobalEntityIndex;
use crate::world::update::mut_channel::{DirtyNotifier, DirtySet, MutReceiver};

// Diagnostic counters for the perf-upgrade project. These measure how much
// work `dirty_receiver_candidates` does per invocation. Phase 3 / C.4 landed
// the dirty-push model via `DirtySet::build_candidates`; `receivers_visited`
// on idle ticks (no component mutations) is now zero. Enabled via `bench_instrumentation`.
/// Diagnostic counters for the `dirty_receiver_candidates` scan.
#[cfg(feature = "bench_instrumentation")]
pub mod dirty_scan_counters {
    use std::sync::atomic::{AtomicU64, Ordering};
    #[doc(hidden)]
    pub static SCAN_CALLS: AtomicU64 = AtomicU64::new(0);
    #[doc(hidden)]
    pub static RECEIVERS_VISITED: AtomicU64 = AtomicU64::new(0);
    #[doc(hidden)]
    pub static DIRTY_RESULTS: AtomicU64 = AtomicU64::new(0);

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
            .unwrap_or_default();
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
        // `>` vs `>=` is an equivalent mutation: `resize_with` to the length the
        // Vec already has is a no-op, so the two agree on every input.
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
            warn!(
                "UserDiffHandler: kind_bit unresolved for {:?}; notifier not attached",
                component_kind
            );
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
        // Defensive only, and unreachable in practice: `kinds_by_bit` is sized
        // to the protocol's `kind_count` at construction and the protocol is
        // locked before any connection exists, so `bit_idx < len` always holds
        // and this branch never runs. The `bit_idx + 1` inside it is therefore
        // untestable by any honest test -- kept because tolerating a late
        // registration is cheaper than the panic that would otherwise follow.
        if bit_idx >= self.kinds_by_bit.len() {
            self.kinds_by_bit.resize(bit_idx + 1, None);
        }
        if self.kinds_by_bit[bit_idx].is_none() {
            self.kinds_by_bit[bit_idx] = Some(*component_kind);
        }

        // Server path: dirty_set is None — pass a dead Weak so DirtyNotifier's
        // set.upgrade() returns None and push/cancel are no-ops.
        let dirty_set_weak = self
            .dirty_set
            .as_ref()
            .map(Arc::downgrade)
            .unwrap_or_default();
        receiver.attach_notifier(DirtyNotifier::new(
            entity_idx,
            kind_bit,
            dirty_set_weak,
            self.global_dirty.clone(),
        ));

        self.ensure_dense_capacity(entity_idx);
        let slot = self.slot(entity_idx, kind_bit);
        self.receivers_dense[slot] = Some(receiver);
        self.entity_kind_to_key
            .insert((*entity, *component_kind), (entity_idx, kind_bit));
    }

    pub fn deregister_component(&mut self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        let Some((entity_idx, kind_bit)) =
            self.entity_kind_to_key.remove(&(*entity, *component_kind))
        else {
            // Never registered (or already deregistered) — nothing to clean up.
            return;
        };
        let slot = self.slot(entity_idx, kind_bit);
        // Defensive bound. `ensure_dense_capacity` grew the array to
        // `(entity_idx + 1) * kind_count` at registration, and
        // `slot <= (entity_idx + 1) * kind_count - 1`, so a registered pair is
        // always strictly in bounds; `<` and `<=` cannot be distinguished.
        if slot < self.receivers_dense.len() {
            // Clear the mask BEFORE dropping the receiver. `GlobalDirtyBitset` is a
            // refcount matrix whose invariant is `ref_count > 0 ↔ dirty bit set`, and
            // `MutReceiver::clear_mask` → `notify_clean` → `decrement` is its ONLY
            // decrement path. Dropping a receiver whose mask is still dirty therefore
            // leaks that refcount permanently: the bit stays set at
            // `(entity_idx, kind_bit)` with nothing left able to clear it. Because
            // `GlobalEntityIndex` is recyclable and the per-user update plan is built
            // index-keyed from the frozen bitset, the next entity to occupy this index
            // inherits the dead component's dirty bit and is asked to serialize a kind
            // it never had — wasted framing at best, and another entity's update at
            // worst. `clear_mask` is a no-op when the mask is already clean.
            if let Some(receiver) = &self.receivers_dense[slot] {
                receiver.clear_mask();
            }
            self.receivers_dense[slot] = None;
        }

        // Only the client path has a DirtySet to cancel from. (The server path's
        // equivalent is the `clear_mask` above, which reaches `global_dirty` through
        // the receiver's notifier.)
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
        let (entity_idx, kind_bit) = self
            .entity_kind_to_key
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
        let Some((entity_idx, kind_bit)) = self
            .entity_kind_to_key
            .get(&(*entity, *component_kind))
            .copied()
        else {
            return true;
        };
        let slot = self.slot(entity_idx, kind_bit);
        match self.receivers_dense.get(slot) {
            Some(Some(r)) => r.diff_mask_is_clear(),
            _ => true,
        }
    }

    /// Sets every dirty bit for `(entity, component_kind)`, forcing a
    /// full-state update. Called when authority over a delegated entity is
    /// granted: optimistic mutations made between the authority request and
    /// the grant fanned out before this receiver existed and were lost, so
    /// the new authority publishes its complete component state once.
    pub fn mark_receiver_fully_dirty(&self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        let Some((entity_idx, kind_bit)) = self
            .entity_kind_to_key
            .get(&(*entity, *component_kind))
            .copied()
        else {
            return;
        };
        let slot = self.slot(entity_idx, kind_bit);
        if let Some(Some(receiver)) = self.receivers_dense.get(slot) {
            receiver.mark_all_dirty();
        }
    }

    /// Marks the receiver for `(entity, component_kind)` as delivered.
    /// Called by the delivery-confirmation path when a spawn/insert ACK arrives.
    pub fn mark_receiver_delivered(&self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        let Some((entity_idx, kind_bit)) = self
            .entity_kind_to_key
            .get(&(*entity, *component_kind))
            .copied()
        else {
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
        let Some((entity_idx, kind_bit)) = self
            .entity_kind_to_key
            .get(&(*entity, *component_kind))
            .copied()
        else {
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
        &self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
        other_mask: &DiffMask,
    ) {
        let (entity_idx, kind_bit) = self
            .entity_kind_to_key
            .get(&(*entity, *component_kind))
            .copied()
            .expect("Should not call this unless we're sure there's a receiver");
        let slot = self.slot(entity_idx, kind_bit);
        let Some(Some(receiver)) = self.receivers_dense.get(slot) else {
            panic!("Should not call this unless we're sure there's a receiver");
        };
        receiver.or_mask(other_mask);
    }

    pub fn clear_diff_mask(&self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        let (entity_idx, kind_bit) = self
            .entity_kind_to_key
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
    pub fn clear_diff_mask_fast(&self, entity_idx: GlobalEntityIndex, kind_bit: u16) {
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
        self.receivers_dense
            .iter()
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
                    // `word_idx * 64` is only distinguishable from a mutated
                    // `word_idx / 64` once `word_idx >= 1`, which needs a
                    // protocol with more than 64 component kinds. Reaching it
                    // would mean standing up 65 `Replicate` types purely to move
                    // a mutation score, so it is left uncovered deliberately.
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
                assert!(
                    seen.insert(slot),
                    "collision at entity={entity_raw} kind_bit={kind_bit}"
                );
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
                assert!(
                    slot < vec.len(),
                    "slot {slot} out of bounds after grow for entity={entity_raw}"
                );
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
            if s < vec.len() {
                vec[s] = None;
            }
        }

        // B gets the recycled index 3.
        let idx_b = GlobalEntityIndex(3);
        let slot_b = idx_b.as_usize() * kind_count + 2;
        assert!(
            vec[slot_b].is_none(),
            "slot must be None after free, not alias A's value"
        );

        // Register B at the same slot.
        vec[slot_b] = Some(99u32);
        assert_eq!(vec[slot_b], Some(99u32));
    }
}

#[cfg(test)]
mod user_diff_handler_tests {
    //! Two concerns, one fixture (a real `UserDiffHandler` over a real
    //! `GlobalDiffHandler` / `GlobalDirtyBitset`, with two component kinds so
    //! the dense stride is greater than one):
    //!
    //! 1. The `GlobalDirtyBitset` refcount invariant across deregistration,
    //!    documented below.
    //! 2. The dense slot arithmetic — `slot()` and the four `_fast` methods
    //!    that inline their own copy of it — and the client-side dirty
    //!    candidate bit decode.
    //!
    //! ## The refcount invariant (world editor §69p, 2026-08-10)
    //!
    //! `GlobalDirtyBitset` is a refcount matrix: `ref_count > 0 ↔ dirty bit set`.
    //! Its ONLY decrement path is `MutReceiver::clear_mask` → `notify_clean` →
    //! `decrement`. `deregister_component` used to drop the receiver without
    //! clearing its mask, so a component removed while dirty leaked its refcount
    //! permanently — the bit stayed set with nothing left able to clear it. Since
    //! `GlobalEntityIndex` is recyclable and the per-user update plan is built
    //! index-keyed from the frozen bitset, the next entity to occupy that index
    //! inherited a dead component's dirty bit and was asked to serialize a kind it
    //! never had. Measured in the wild as 649 skipped entities across 649 distinct
    //! indices, every one planning a kind the live entity did not hold.
    //!
    //! These drive the REAL `UserDiffHandler` / `GlobalDiffHandler` /
    //! `GlobalDirtyBitset` on the server path (a present `GlobalDirtyBitset`, so
    //! `dirty_set` is `None` — exactly the path whose cleanup was missing).

    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::{Arc, RwLock};

    use super::*;
    use crate::bigmap::BigMapKey;
    use crate::world::component::property::Property;
    use crate::world::delegation::auth_channel::EntityAuthAccessor;
    use crate::world::update::mut_channel::{MutChannelType, MutReceiver};
    use crate::{ComponentKinds, InScopeEntities, PropertyMutator, Replicate};

    #[derive(Replicate)]
    struct Ghost {
        value: Property<u8>,
    }

    /// A second kind, so `kind_count` is 2 and the slot formula
    /// `entity_idx * kind_count + kind_bit` is actually distinguishable from
    /// the arithmetic variations a mutation of it would produce. With a single
    /// kind the stride is 1 and every wrong formula still collides onto the
    /// right slot.
    #[derive(Replicate)]
    struct Phantom {
        value: Property<u8>,
    }

    /// Mirrors the server's `MutChannelData`: one receiver per address, cached,
    /// with `send` fanning mutations out to all of them.
    struct TestMutChannel {
        diff_mask_length: u8,
        receivers: Vec<MutReceiver>,
        receiver_index: HashMap<SocketAddr, usize>,
    }

    impl MutChannelType for TestMutChannel {
        fn new_receiver(&mut self, address_opt: &Option<SocketAddr>) -> Option<MutReceiver> {
            let address = address_opt.expect("test channel requires an address");
            if let Some(&idx) = self.receiver_index.get(&address) {
                return Some(self.receivers[idx].clone());
            }
            let receiver = MutReceiver::new(self.diff_mask_length);
            let idx = self.receivers.len();
            self.receivers.push(receiver.clone());
            self.receiver_index.insert(address, idx);
            Some(receiver)
        }

        fn send(&self, property_index: u8) {
            for receiver in &self.receivers {
                receiver.mutate(property_index);
            }
        }
    }

    struct TestGwm {
        diff_handler: Arc<RwLock<GlobalDiffHandler>>,
        global_dirty: Arc<GlobalDirtyBitset>,
        /// When false the manager presents itself as a client: no global dirty
        /// bitset, which is the condition under which `UserDiffHandler`
        /// allocates its per-user `DirtySet` and `dirty_receiver_candidates`
        /// reports anything at all.
        has_global_dirty: bool,
    }

    impl InScopeEntities<GlobalEntity> for TestGwm {
        fn has_entity(&self, _: &GlobalEntity) -> bool {
            true
        }
    }

    impl GlobalWorldManagerType for TestGwm {
        fn component_kinds(&self, _: &GlobalEntity) -> Option<Vec<ComponentKind>> {
            None
        }
        fn entity_can_relate_to_user(&self, _: &GlobalEntity, _: &u64) -> bool {
            true
        }
        fn new_mut_channel(&self, diff_mask_length: u8) -> Arc<RwLock<dyn MutChannelType>> {
            Arc::new(RwLock::new(TestMutChannel {
                diff_mask_length,
                receivers: Vec::new(),
                receiver_index: HashMap::new(),
            }))
        }
        fn diff_handler(&self) -> Arc<RwLock<GlobalDiffHandler>> {
            self.diff_handler.clone()
        }
        fn register_component(
            &self,
            _: &ComponentKinds,
            _: &GlobalEntity,
            _: &ComponentKind,
            _: u8,
        ) -> PropertyMutator {
            unreachable!("not exercised by these tests")
        }
        fn get_entity_auth_accessor(&self, _: &GlobalEntity) -> EntityAuthAccessor {
            unreachable!("not exercised by these tests")
        }
        fn entity_needs_mutator_for_delegation(&self, _: &GlobalEntity) -> bool {
            false
        }
        fn entity_is_replicating(&self, _: &GlobalEntity) -> bool {
            true
        }
        fn entity_is_static(&self, _: &GlobalEntity) -> bool {
            false
        }
        fn global_dirty_bitset(&self) -> Option<Arc<GlobalDirtyBitset>> {
            if !self.has_global_dirty {
                return None;
            }
            Some(self.global_dirty.clone())
        }
    }

    struct Fixture {
        gwm: TestGwm,
        kinds: ComponentKinds,
        addr: Option<SocketAddr>,
        kind: ComponentKind,
        kind2: ComponentKind,
    }

    fn fixture() -> Fixture {
        build_fixture(true)
    }

    /// A fixture whose world manager reports no global dirty bitset, so the
    /// `UserDiffHandler` under test takes the client path and populates a
    /// per-user `DirtySet`.
    fn client_fixture() -> Fixture {
        build_fixture(false)
    }

    fn build_fixture(has_global_dirty: bool) -> Fixture {
        let mut kinds = ComponentKinds::new();
        kinds.add_component::<Ghost>();
        kinds.add_component::<Phantom>();

        let diff_handler = Arc::new(RwLock::new(GlobalDiffHandler::new()));
        diff_handler
            .write()
            .unwrap()
            .set_protocol_kind_count(kinds.kind_count());
        let global_dirty = Arc::new(GlobalDirtyBitset::new(64, kinds.kind_count() as usize));

        Fixture {
            gwm: TestGwm {
                diff_handler,
                global_dirty,
                has_global_dirty,
            },
            kinds,
            addr: Some("127.0.0.1:4000".parse().unwrap()),
            kind: ComponentKind::of::<Ghost>(),
            kind2: ComponentKind::of::<Phantom>(),
        }
    }

    /// Allocates `entity` and registers `kind` on it with both handlers,
    /// without dirtying it. Returns the dense coordinates the pair landed on.
    fn register(
        fx: &Fixture,
        udh: &mut UserDiffHandler,
        entity: GlobalEntity,
        kind: &ComponentKind,
    ) -> (GlobalEntityIndex, u16) {
        let (idx, kind_bit) = {
            let mut gdh = fx.gwm.diff_handler.write().unwrap();
            let idx = gdh
                .entity_to_global_idx(&entity)
                .unwrap_or_else(|| gdh.alloc_entity(entity));
            gdh.register_component(&fx.kinds, &fx.gwm, &entity, kind, 1);
            let kind_bit = gdh.kind_bit(kind).expect("kind_bit must resolve");
            (idx, kind_bit)
        };
        udh.register_component(&fx.addr, &entity, kind);
        (idx, kind_bit)
    }

    /// Dirties an already-registered `(entity, kind)` pair through the real
    /// mutation fan-out.
    fn dirty(fx: &Fixture, entity: GlobalEntity, kind: &ComponentKind) {
        fx.gwm
            .diff_handler
            .read()
            .unwrap()
            .receiver(&fx.addr, &entity, kind)
            .expect("receiver must exist after registration")
            .mutate(0);
    }

    /// Allocates `entity`, registers `Ghost` on it with both handlers, dirties it,
    /// and returns the index it landed on. Asserts the bit is genuinely set.
    fn register_and_dirty(
        fx: &Fixture,
        udh: &mut UserDiffHandler,
        entity: GlobalEntity,
    ) -> (GlobalEntityIndex, u16) {
        let (idx, kind_bit) = {
            let mut gdh = fx.gwm.diff_handler.write().unwrap();
            let idx = gdh.alloc_entity(entity);
            gdh.register_component(&fx.kinds, &fx.gwm, &entity, &fx.kind, 1);
            let kind_bit = gdh.kind_bit(&fx.kind).expect("kind_bit must resolve");
            (idx, kind_bit)
        };
        udh.register_component(&fx.addr, &entity, &fx.kind);

        let receiver = fx
            .gwm
            .diff_handler
            .read()
            .unwrap()
            .receiver(&fx.addr, &entity, &fx.kind)
            .expect("receiver must exist after registration");
        receiver.mutate(0);

        // Anti-vacuity: the rest of each test is meaningless if this never fired.
        assert!(
            fx.gwm.global_dirty.is_component_dirty(idx, kind_bit),
            "the fixture failed to make the component dirty in the first place"
        );
        (idx, kind_bit)
    }

    #[test]
    fn deregistering_a_dirty_component_releases_its_global_refcount() {
        let fx = fixture();
        let mut udh = UserDiffHandler::new(&fx.gwm);
        let entity = GlobalEntity::from_u64(1);

        let (idx, kind_bit) = register_and_dirty(&fx, &mut udh, entity);

        udh.deregister_component(&entity, &fx.kind);

        assert!(
            !fx.gwm.global_dirty.is_component_dirty(idx, kind_bit),
            "deregistering a DIRTY component leaked its GlobalDirtyBitset refcount — \
             the bit is still set with no receiver left that could ever clear it"
        );
    }

    #[test]
    fn a_recycled_index_does_not_inherit_a_ghost_kind() {
        let fx = fixture();
        let mut udh = UserDiffHandler::new(&fx.gwm);

        let entity_a = GlobalEntity::from_u64(1);
        let (idx_a, kind_bit) = register_and_dirty(&fx, &mut udh, entity_a);

        // Despawn A while it is still dirty, exactly as the server does.
        udh.deregister_component(&entity_a, &fx.kind);
        {
            let mut gdh = fx.gwm.diff_handler.write().unwrap();
            gdh.deregister_component(&entity_a, &fx.kind);
            gdh.free_entity(&entity_a);
        }

        // B takes over A's index.
        let entity_b = GlobalEntity::from_u64(2);
        let idx_b = fx.gwm.diff_handler.write().unwrap().alloc_entity(entity_b);

        // Anti-vacuity: without recycling this test proves nothing.
        assert_eq!(
            idx_a, idx_b,
            "the index was not recycled, so this test cannot observe inheritance"
        );

        assert!(
            !fx.gwm.global_dirty.is_component_dirty(idx_b, kind_bit),
            "a recycled index inherited the despawned component's dirty bit — the next \
             update plan built from this index would name a kind the new entity never held"
        );
    }

    // -- dense slot arithmetic --------------------------------------------
    //
    // `slot()` is `entity_idx * kind_count + kind_bit`, and the four `_fast`
    // methods each inline their own copy of it. Every one of these tests needs
    // at least two kinds AND at least two entity indices: at `kind_count == 1`
    // the stride collapses and a wrong formula still lands on the right slot.

    /// Registers `Ghost` and `Phantom` on three entities and returns the dense
    /// coordinates of each pair, asserting they are all distinct.
    fn register_grid(
        fx: &Fixture,
        udh: &mut UserDiffHandler,
    ) -> Vec<(GlobalEntity, ComponentKind, GlobalEntityIndex, u16)> {
        let mut grid = Vec::new();
        for raw in 1u64..=3 {
            let entity = GlobalEntity::from_u64(raw);
            for kind in [fx.kind, fx.kind2] {
                let (idx, kind_bit) = register(fx, udh, entity, &kind);
                grid.push((entity, kind, idx, kind_bit));
            }
        }
        let mut coords: Vec<_> = grid.iter().map(|(_, _, i, b)| (*i, *b)).collect();
        coords.sort_by_key(|(i, b)| (i.as_usize(), *b));
        coords.dedup();
        assert_eq!(
            coords.len(),
            grid.len(),
            "the fixture handed out a duplicate (entity_idx, kind_bit) pair, so \
             nothing below can distinguish a slot collision"
        );
        assert!(
            coords.iter().any(|(i, _)| i.as_usize() >= 2),
            "at least one entity must land at index 2 or higher, or the dense \
             array never has to grow past its first block"
        );
        assert!(
            coords.iter().any(|(_, b)| *b != 0),
            "at least one kind must land at a nonzero kind_bit"
        );
        grid
    }

    #[test]
    fn a_dirty_component_does_not_make_its_neighbours_look_dirty() {
        let fx = fixture();
        let mut udh = UserDiffHandler::new(&fx.gwm);
        let grid = register_grid(&fx, &mut udh);

        // Pick the pair furthest from the origin, so a collapsed slot formula
        // has somewhere wrong to land.
        let (target_entity, target_kind, target_idx, target_bit) = *grid
            .iter()
            .max_by_key(|(_, _, idx, bit)| (idx.as_usize(), *bit))
            .unwrap();

        for (entity, kind, _, _) in &grid {
            udh.mark_receiver_delivered(entity, kind);
        }
        dirty(&fx, target_entity, &target_kind);

        for (entity, kind, idx, bit) in &grid {
            let is_target = *idx == target_idx && *bit == target_bit;
            assert_eq!(
                udh.is_receiver_dirty_and_delivered_fast(*idx, *bit),
                is_target,
                "dirty+delivered at ({idx:?}, {bit}) must be {is_target}: only \
                 the mutated pair may report dirty",
            );
            assert_eq!(
                udh.diff_mask_is_clear_fast(*idx, *bit),
                !is_target,
                "mask-clear at ({idx:?}, {bit}) must be {}",
                !is_target,
            );
            assert_eq!(
                udh.is_receiver_dirty_and_delivered(entity, kind),
                is_target,
                "the cold path must agree with the fast path at ({idx:?}, {bit})",
            );
            assert_eq!(
                udh.diff_mask_is_clear(entity, kind),
                !is_target,
                "the cold path must agree with the fast path at ({idx:?}, {bit})",
            );
        }
    }

    #[test]
    fn the_fast_snapshot_returns_the_same_mask_as_the_cold_one() {
        let fx = fixture();
        let mut udh = UserDiffHandler::new(&fx.gwm);
        let grid = register_grid(&fx, &mut udh);

        // Dirty every pair, so each receiver has a mask worth comparing.
        for (entity, kind, _, _) in &grid {
            dirty(&fx, *entity, kind);
        }
        for (entity, kind, idx, bit) in &grid {
            assert_eq!(
                udh.diff_mask_snapshot_fast(*idx, *bit),
                Some(udh.diff_mask_snapshot(entity, kind)),
                "fast and cold snapshots must resolve to the same receiver at \
                 ({idx:?}, {bit})",
            );
        }
    }

    #[test]
    fn the_fast_snapshot_reports_nothing_for_an_unregistered_slot() {
        let fx = fixture();
        let mut udh = UserDiffHandler::new(&fx.gwm);
        let grid = register_grid(&fx, &mut udh);
        let beyond = GlobalEntityIndex(
            grid.iter().map(|(_, _, i, _)| i.as_usize()).max().unwrap() as u32 + 4,
        );
        assert_eq!(udh.diff_mask_snapshot_fast(beyond, 0), None);
        assert!(udh.diff_mask_is_clear_fast(beyond, 0));
        assert!(!udh.is_receiver_dirty_and_delivered_fast(beyond, 0));
    }

    #[test]
    fn clearing_one_mask_leaves_the_other_components_dirty() {
        let fx = fixture();
        let mut udh = UserDiffHandler::new(&fx.gwm);
        let grid = register_grid(&fx, &mut udh);

        for (entity, kind, _, _) in &grid {
            dirty(&fx, *entity, kind);
        }
        let (_, _, target_idx, target_bit) = *grid
            .iter()
            .max_by_key(|(_, _, idx, bit)| (idx.as_usize(), *bit))
            .unwrap();
        udh.clear_diff_mask_fast(target_idx, target_bit);

        for (_, _, idx, bit) in &grid {
            let is_target = *idx == target_idx && *bit == target_bit;
            assert_eq!(
                udh.diff_mask_is_clear_fast(*idx, *bit),
                is_target,
                "clearing ({target_idx:?}, {target_bit}) must not reach \
                 ({idx:?}, {bit})",
            );
        }
    }

    #[test]
    fn registration_is_tracked_and_reversed_per_component() {
        let fx = fixture();
        let mut udh = UserDiffHandler::new(&fx.gwm);
        let grid = register_grid(&fx, &mut udh);

        for (entity, kind, _, _) in &grid {
            assert!(udh.has_component(entity, kind));
        }
        let (entity, kind, _, _) = grid[0];
        udh.deregister_component(&entity, &kind);
        assert!(!udh.has_component(&entity, &kind));
        for (other_entity, other_kind, _, _) in &grid[1..] {
            assert!(
                udh.has_component(other_entity, other_kind),
                "deregistering one pair must not disturb the others",
            );
        }
        // Deregistering again is a no-op rather than a panic.
        udh.deregister_component(&entity, &kind);
    }

    #[test]
    fn an_unregistered_component_reads_as_clean_and_ignores_writes() {
        let fx = fixture();
        let udh = UserDiffHandler::new(&fx.gwm);
        let entity = GlobalEntity::from_u64(9);
        assert!(!udh.has_component(&entity, &fx.kind));
        assert!(udh.diff_mask_is_clear(&entity, &fx.kind));
        assert!(!udh.is_receiver_dirty_and_delivered(&entity, &fx.kind));
        // Neither of these has a receiver to reach; both must simply return.
        udh.mark_receiver_fully_dirty(&entity, &fx.kind);
        udh.mark_receiver_delivered(&entity, &fx.kind);
    }

    #[test]
    fn marking_a_receiver_fully_dirty_dirties_only_that_component() {
        let fx = fixture();
        let mut udh = UserDiffHandler::new(&fx.gwm);
        let grid = register_grid(&fx, &mut udh);
        let (target_entity, target_kind, target_idx, target_bit) = *grid
            .iter()
            .max_by_key(|(_, _, idx, bit)| (idx.as_usize(), *bit))
            .unwrap();

        udh.mark_receiver_fully_dirty(&target_entity, &target_kind);

        for (_, _, idx, bit) in &grid {
            let is_target = *idx == target_idx && *bit == target_bit;
            assert_eq!(
                udh.diff_mask_is_clear_fast(*idx, *bit),
                !is_target,
                "only ({target_idx:?}, {target_bit}) should have been dirtied",
            );
        }
    }

    // -- client path: dirty candidate decoding -----------------------------

    #[test]
    fn dirty_candidates_name_exactly_the_mutated_components() {
        let fx = client_fixture();
        let mut udh = UserDiffHandler::new(&fx.gwm);
        let entity = GlobalEntity::from_u64(1);
        let (_, ghost_bit) = register(&fx, &mut udh, entity, &fx.kind);
        let (_, phantom_bit) = register(&fx, &mut udh, entity, &fx.kind2);

        // Anti-vacuity: the bit-decode loop below is only interesting if the
        // two kinds sit at different bit positions, one of them nonzero.
        assert_ne!(ghost_bit, phantom_bit);
        assert!(ghost_bit != 0 || phantom_bit != 0);

        assert!(
            udh.dirty_receiver_candidates().is_empty(),
            "nothing has been mutated yet",
        );

        dirty(&fx, entity, &fx.kind2);

        let candidates = udh.dirty_receiver_candidates();
        let kinds = candidates
            .get(&entity)
            .expect("the mutated entity must appear as a candidate");
        assert!(
            kinds.contains(&fx.kind2),
            "the mutated kind must be decoded back out of its dirty bit",
        );
        assert!(
            !kinds.contains(&fx.kind),
            "an untouched kind must not be reported dirty -- a wrong bit-to-kind \
             decode shows up exactly here",
        );
        assert_eq!(candidates.len(), 1);
    }

    #[test]
    fn dirty_candidates_decode_every_mutated_kind_on_an_entity() {
        let fx = client_fixture();
        let mut udh = UserDiffHandler::new(&fx.gwm);
        let entity = GlobalEntity::from_u64(1);
        register(&fx, &mut udh, entity, &fx.kind);
        register(&fx, &mut udh, entity, &fx.kind2);

        dirty(&fx, entity, &fx.kind);
        dirty(&fx, entity, &fx.kind2);

        let candidates = udh.dirty_receiver_candidates();
        let kinds = candidates.get(&entity).expect("entity must be a candidate");
        assert_eq!(
            kinds.len(),
            2,
            "both mutated kinds must be decoded out of the same dirty word",
        );
        assert!(kinds.contains(&fx.kind) && kinds.contains(&fx.kind2));
    }

    #[test]
    fn the_server_path_reports_no_dirty_candidates() {
        let fx = fixture();
        let mut udh = UserDiffHandler::new(&fx.gwm);
        let entity = GlobalEntity::from_u64(1);
        register(&fx, &mut udh, entity, &fx.kind);
        dirty(&fx, entity, &fx.kind);
        assert!(
            udh.dirty_receiver_candidates().is_empty(),
            "the server drives candidate selection from GlobalDirtyBitset, so this \
             path must stay a no-op even with a genuinely dirty component",
        );
    }
}
