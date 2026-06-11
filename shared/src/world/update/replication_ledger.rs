use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::RwLock,
};

use crate::world::update::user_diff_handler::UserDiffHandler;
use crate::{
    ComponentKind, DiffMask, GlobalEntity, GlobalEntityIndex, GlobalWorldManagerType,
};

/// Per-user replication diff-state, lifted out of the `&mut`-owned send
/// connection into an `Arc`-shareable, lock-free structure (MISSION_TICK_FLOOR
/// Lever 3 / L3 send-state seam).
///
/// Wraps the per-user [`UserDiffHandler`] (the `MutReceiver` container holding
/// per-property `AtomicDiffMask`s + `delivered` flags). Every entry op is a
/// `&self` atomic; the only `&mut` is `register_component` /
/// `deregister_component`, which grow the receiver container (a `resize_with`
/// that reallocates and would invalidate a concurrent reader). An `RwLock`
/// guards exactly that hazard (decision A): registration takes the write guard
/// (a rare scope/spawn boundary, worker idle); send-time access takes the read
/// guard then atomic-ops the entries.
///
/// In the seam the park still serializes all access (Sim-set via the
/// `MutChannel`'s own `Arc<AtomicDiffMask>` clones — which never touch this
/// lock — vs prepare-clear vs drop-`or`), so the lock is uncontended. It is the
/// structural prerequisite for the L3.4 free-running transmit, where prepare and
/// the worker reach the *same* ledger via separate `Arc` handles.
pub struct ReplicationLedger {
    handler: RwLock<UserDiffHandler>,
}

impl ReplicationLedger {
    pub fn new(global_world_manager: &dyn GlobalWorldManagerType) -> Self {
        Self {
            handler: RwLock::new(UserDiffHandler::new(global_world_manager)),
        }
    }

    // ── Container growth — write guard (boundary: scope/spawn, worker idle) ──

    pub fn register_component(
        &self,
        address: &Option<SocketAddr>,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) {
        self.handler
            .write()
            .expect("ReplicationLedger lock poisoned")
            .register_component(address, entity, component_kind);
    }

    pub fn deregister_component(&self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        self.handler
            .write()
            .expect("ReplicationLedger lock poisoned")
            .deregister_component(entity, component_kind);
    }

    // ── Entry ops — read guard + `&self` atomic ─────────────────────────────

    pub fn has_component(&self, entity: &GlobalEntity, component_kind: &ComponentKind) -> bool {
        self.read().has_component(entity, component_kind)
    }

    pub fn or_diff_mask(
        &self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
        new_diff_mask: &DiffMask,
    ) {
        self.read().or_diff_mask(entity, component_kind, new_diff_mask);
    }

    pub fn diff_mask_snapshot(
        &self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) -> DiffMask {
        self.read().diff_mask_snapshot(entity, component_kind)
    }

    pub fn clear_diff_mask(&self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        self.read().clear_diff_mask(entity, component_kind);
    }

    pub fn mark_receiver_delivered(&self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        self.read().mark_receiver_delivered(entity, component_kind);
    }

    pub fn mark_receiver_fully_dirty(&self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        self.read().mark_receiver_fully_dirty(entity, component_kind);
    }

    pub fn is_receiver_dirty_and_delivered(
        &self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) -> bool {
        self.read().is_receiver_dirty_and_delivered(entity, component_kind)
    }

    pub fn is_receiver_dirty_and_delivered_fast(
        &self,
        entity_idx: GlobalEntityIndex,
        kind_bit: u16,
    ) -> bool {
        self.read()
            .is_receiver_dirty_and_delivered_fast(entity_idx, kind_bit)
    }

    pub fn diff_mask_is_clear_fast(&self, entity_idx: GlobalEntityIndex, kind_bit: u16) -> bool {
        self.read().diff_mask_is_clear_fast(entity_idx, kind_bit)
    }

    pub fn diff_mask_snapshot_fast(
        &self,
        entity_idx: GlobalEntityIndex,
        kind_bit: u16,
    ) -> Option<DiffMask> {
        self.read().diff_mask_snapshot_fast(entity_idx, kind_bit)
    }

    pub fn clear_diff_mask_fast(&self, entity_idx: GlobalEntityIndex, kind_bit: u16) {
        self.read().clear_diff_mask_fast(entity_idx, kind_bit);
    }

    pub fn diff_mask_is_clear(&self, entity: &GlobalEntity, component_kind: &ComponentKind) -> bool {
        self.read().diff_mask_is_clear(entity, component_kind)
    }

    pub fn dirty_receiver_candidates(&self) -> HashMap<GlobalEntity, HashSet<ComponentKind>> {
        self.read().dirty_receiver_candidates()
    }

    #[cfg(feature = "test_utils")]
    pub fn receiver_count(&self) -> usize {
        self.read().receiver_count()
    }

    #[cfg(feature = "test_utils")]
    pub fn dirty_candidates_count(&self) -> usize {
        self.read().dirty_candidates_count()
    }

    #[inline]
    fn read(&self) -> std::sync::RwLockReadGuard<'_, UserDiffHandler> {
        self.handler.read().expect("ReplicationLedger lock poisoned")
    }

    /// Acquire the read guard explicitly, so a hot batch of entry ops (e.g.
    /// `prepare_send_job`'s per-user Phase 3A gate loop) can take ONE coarse
    /// guard instead of one per call. Callers operate directly on the returned
    /// [`UserDiffHandler`] (its entry ops are `&self` atomic). Hold it only
    /// across reads/atomic-ops — never across `register_component`
    /// /`deregister_component` (write guard) on the same ledger (deadlock).
    pub fn read_guard(&self) -> std::sync::RwLockReadGuard<'_, UserDiffHandler> {
        self.read()
    }
}
