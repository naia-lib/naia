use std::{
    hash::Hash,
    sync::{Arc, RwLock},
};

use crate::{
    world::host::global_entity_record::GlobalEntityRecord, ComponentKind, EntityDoesNotExistError,
    EntityHandle, EntityHandleConverter, GlobalDiffHandler, PropertyMutator, Replicate,
    WorldRecord,
};

pub struct HostGlobalWorldManager<E: Copy + Eq + Hash + Send + Sync> {
    diff_handler: Arc<RwLock<GlobalDiffHandler<E>>>,
    world_record: WorldRecord<E>,
}

impl<E: Copy + Eq + Hash + Send + Sync> HostGlobalWorldManager<E> {
    pub fn new() -> Self {
        Self {
            diff_handler: Arc::new(RwLock::new(GlobalDiffHandler::new())),
            world_record: WorldRecord::new(),
        }
    }

    // Accessors
    pub fn world_record(&self) -> &WorldRecord<E> {
        &self.world_record
    }

    pub fn diff_handler(&self) -> &Arc<RwLock<GlobalDiffHandler<E>>> {
        &self.diff_handler
    }

    // Entities
    pub fn entities(&self) -> Vec<E> {
        self.world_record.entities()
    }

    pub fn has_entity(&self, entity: &E) -> bool {
        self.world_record.has_entity(entity)
    }

    // Spawn
    pub fn spawn_entity(&mut self, entity: &E) {
        self.world_record.spawn_entity(entity)
    }

    // Despawn
    pub fn despawn_entity(&mut self, entity: &E) -> Option<GlobalEntityRecord> {
        // Clean up associated components
        for component_kind in self.component_kinds(entity).unwrap() {
            self.remove_component(entity, &component_kind);
        }

        // Despawn from World Record
        self.world_record.despawn_entity(entity)
    }

    // Component Kinds
    pub fn component_kinds(&self, entity: &E) -> Option<Vec<ComponentKind>> {
        self.world_record.component_kinds(entity)
    }

    // Insert Component
    pub fn insert_component(&mut self, entity: &E, component: &mut dyn Replicate) {
        let component_kind = component.kind();
        let diff_mask_length: u8 = component.diff_mask_size();

        self.world_record.add_component(entity, &component_kind);

        let mut_sender = self
            .diff_handler
            .as_ref()
            .write()
            .expect("DiffHandler should be initialized")
            .register_component(entity, &component_kind, diff_mask_length);

        let prop_mutator = PropertyMutator::new(mut_sender);

        component.set_mutator(&prop_mutator);
    }

    // Remove Component
    pub fn remove_component(&mut self, entity: &E, component_kind: &ComponentKind) {
        self.world_record.remove_component(entity, component_kind);
        self.diff_handler
            .as_ref()
            .write()
            .expect("Haven't initialized DiffHandler")
            .deregister_component(entity, component_kind);
    }
}

impl<E: Copy + Eq + Hash + Send + Sync> EntityHandleConverter<E> for HostGlobalWorldManager<E> {
    // Conversions
    fn handle_to_entity(&self, handle: &EntityHandle) -> E {
        self.world_record.handle_to_entity(handle)
    }

    fn entity_to_handle(&self, entity: &E) -> Result<EntityHandle, EntityDoesNotExistError> {
        self.world_record.entity_to_handle(entity)
    }
}
