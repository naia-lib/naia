use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    net::SocketAddr,
    sync::Arc,
};

use crate::world::update::replication_ledger::ReplicationLedger;
use crate::{
    ComponentKind, DiffMask, EntityAndGlobalEntityConverter, GlobalEntity, GlobalEntityIndex,
    GlobalWorldManagerType, WorldRefType,
};

/// Per-user diff-state accessor (MISSION_TICK_FLOOR Lever 3 / L3 send-state
/// seam). The inline `UserDiffHandler` it used to own now lives behind an
/// `Arc<ReplicationLedger>` (RwLock-guarded, `&self`-atomic), so the diff-state
/// is `Arc`-shareable and the drop path re-sets masks atomically without a
/// `&mut` borrow. This stays a thin per-connection facade so all existing
/// `LocalWorldManager` / `SendState` call sites are unchanged; the only state
/// it still owns directly is the connection `address` (for registration).
pub struct EntityUpdateManager {
    address: Option<SocketAddr>,
    ledger: Arc<ReplicationLedger>,
}

impl EntityUpdateManager {
    pub fn new(
        address: &Option<SocketAddr>,
        global_world_manager: &dyn GlobalWorldManagerType,
    ) -> Self {
        Self {
            address: *address,
            ledger: Arc::new(ReplicationLedger::new(global_world_manager)),
        }
    }

    /// Clone an `Arc` handle to the shared diff-state ledger. The seam keeps a
    /// single strong ref (reached through `send_conn`); L3.4 hands a second
    /// handle to a prepare-side ledger map.
    pub fn ledger(&self) -> Arc<ReplicationLedger> {
        self.ledger.clone()
    }

    pub fn take_outgoing_events<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        &mut self,
        world: &W,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &dyn GlobalWorldManagerType,
        mut updatable_world: HashMap<GlobalEntity, HashSet<ComponentKind>>,
    ) -> HashMap<GlobalEntity, HashSet<ComponentKind>> {
        updatable_world.retain(|global_entity, component_kinds| {
            if !global_world_manager.entity_is_replicating(global_entity) {
                return false;
            }
            let Ok(world_entity) = converter.global_entity_to_entity(global_entity) else {
                panic!(
                    "World Channel: cannot convert global entity ({:?}) to world entity",
                    global_entity
                )
            };
            if !world.has_entity(&world_entity) {
                return false;
            }

            component_kinds.retain(|kind| {
                let has_component = world.has_component_of_kind(&world_entity, kind);
                let diff_mask_clear = self.ledger.diff_mask_is_clear(global_entity, kind);
                has_component && !diff_mask_clear
            });
            !component_kinds.is_empty()
        });
        updatable_world
    }

    // Main

    pub fn diff_handler_has_component(
        &self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) -> bool {
        self.ledger.has_component(entity, component_kind)
    }

    pub fn or_diff_mask(
        &self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
        new_diff_mask: &DiffMask,
    ) {
        self.ledger
            .or_diff_mask(entity, component_kind, new_diff_mask);
    }

    pub fn get_diff_mask(
        &self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) -> DiffMask {
        self.ledger.diff_mask_snapshot(entity, component_kind)
    }

    pub fn clear_diff_mask(&self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        self.ledger.clear_diff_mask(entity, component_kind);
    }

    pub fn register_component(&mut self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        self.ledger
            .register_component(&self.address, entity, component_kind);
    }

    pub fn deregister_component(&mut self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        self.ledger.deregister_component(entity, component_kind);
    }

    /// Marks the receiver for `(entity, component_kind)` as delivered.
    /// Called when the spawn/insert-component ACK arrives, enabling the
    /// Phase 3 fast-path single-lookup check in `is_receiver_dirty_and_delivered`.
    pub fn mark_component_delivered(&self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        self.ledger.mark_receiver_delivered(entity, component_kind);
    }

    /// Phase 3 fast-path: single HashMap lookup that returns `true` iff the
    /// component has pending dirty bits AND its spawn was delivered. Replaces the
    /// 6+ HashMap chain of `is_component_updatable_for_entity` in steady state.
    pub fn is_component_dirty_and_delivered(
        &self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) -> bool {
        self.ledger.is_receiver_dirty_and_delivered(entity, component_kind)
    }

    /// Hot-path version: no GlobalEntity→idx resolution, no RwLock.
    /// Called from Phase 3 inner loop which already has entity_idx + kind_bit.
    pub fn is_component_dirty_and_delivered_fast(
        &self,
        entity_idx: GlobalEntityIndex,
        kind_bit: u16,
    ) -> bool {
        self.ledger.is_receiver_dirty_and_delivered_fast(entity_idx, kind_bit)
    }

    /// Hot-path diff mask clear check: direct compact-key lookup.
    pub fn diff_mask_is_clear_fast(&self, entity_idx: GlobalEntityIndex, kind_bit: u16) -> bool {
        self.ledger.diff_mask_is_clear_fast(entity_idx, kind_bit)
    }

    /// Hot-path mask snapshot: direct compact-key lookup.
    pub fn get_diff_mask_fast(
        &self,
        entity_idx: GlobalEntityIndex,
        kind_bit: u16,
    ) -> Option<DiffMask> {
        self.ledger.diff_mask_snapshot_fast(entity_idx, kind_bit)
    }

    #[cfg(feature = "test_utils")]
    pub fn diff_handler_receiver_count(&self) -> usize {
        self.ledger.receiver_count()
    }

    #[cfg(feature = "test_utils")]
    pub fn dirty_candidates_len(&self) -> usize {
        self.ledger.dirty_candidates_count()
    }

    pub fn build_dirty_candidates_from_receivers(&self) -> HashMap<GlobalEntity, HashSet<ComponentKind>> {
        self.ledger.dirty_receiver_candidates()
    }

    pub fn diff_mask_is_clear(&self, entity: &GlobalEntity, component_kind: &ComponentKind) -> bool {
        self.ledger.diff_mask_is_clear(entity, component_kind)
    }

    /// MISSION_TICK_FLOOR Lever 3: clear the live per-user diff mask up-front
    /// (compact-key). Called from `prepare_send_job` after the frozen
    /// mask has been captured into the plan.
    pub fn clear_diff_mask_fast(&self, entity_idx: GlobalEntityIndex, kind_bit: u16) {
        self.ledger.clear_diff_mask_fast(entity_idx, kind_bit);
    }

}
