use std::{
    hash::Hash,
    sync::{Arc, RwLock},
};

use naia_shared::{
    ComponentKind, EntityDoesNotExistError, EntityHandle, EntityHandleConverter, GlobalDiffHandler,
    GlobalWorldManagerType, MutChannelType, PropertyMutator, Replicate,
};

use super::{global_entity_record::GlobalEntityRecord, world_record::WorldRecord};
use crate::world::mut_channel::MutChannelData;

pub struct GlobalWorldManager<E: Copy + Eq + Hash + Send + Sync> {
    diff_handler: Arc<RwLock<GlobalDiffHandler<E>>>,
    world_record: WorldRecord<E>,
}

impl<E: Copy + Eq + Hash + Send + Sync> GlobalWorldManager<E> {
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
            .register_component(self, entity, &component_kind, diff_mask_length);

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

impl<E: Copy + Eq + Hash + Send + Sync> GlobalWorldManagerType<E> for GlobalWorldManager<E> {
    fn component_kinds(&self, entity: &E) -> Option<Vec<ComponentKind>> {
        self.component_kinds(entity)
    }

    fn to_handle_converter(&self) -> &dyn EntityHandleConverter<E> {
        self
    }

    fn new_mut_channel(&self, diff_mask_length: u8) -> Arc<RwLock<dyn MutChannelType>> {
        let mut_channel = MutChannelData::new(diff_mask_length);
        return Arc::new(RwLock::new(mut_channel));
    }
}

impl<E: Copy + Eq + Hash + Send + Sync> EntityHandleConverter<E> for GlobalWorldManager<E> {
    // Conversions
    fn handle_to_entity(&self, handle: &EntityHandle) -> Result<E, EntityDoesNotExistError> {
        self.world_record.handle_to_entity(handle)
    }

    fn entity_to_handle(&self, entity: &E) -> Result<EntityHandle, EntityDoesNotExistError> {
        self.world_record.entity_to_handle(entity)
    }
}
