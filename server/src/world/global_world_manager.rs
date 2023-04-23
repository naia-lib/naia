use std::{
    collections::HashMap,
    hash::Hash,
    sync::{Arc, RwLock},
};

use naia_shared::{
    BigMap, BigMapKey, ComponentKind, EntityAndGlobalEntityConverter, EntityDoesNotExistError,
    GlobalDiffHandler, GlobalEntity, GlobalWorldManagerType, MutChannelType, PropertyMutator,
    Replicate,
};

use super::global_entity_record::GlobalEntityRecord;
use crate::{world::mut_channel::MutChannelData, EntityOwner, UserKey};

pub struct GlobalWorldManager<E: Copy + Eq + Hash + Send + Sync> {
    diff_handler: Arc<RwLock<GlobalDiffHandler<E>>>,
    /// Information about entities in the internal ECS World
    entity_records: HashMap<E, GlobalEntityRecord>,
    /// Map from the internal [`GlobalEntity`] to the external (e.g. Bevy's) entity id
    global_entity_map: BigMap<GlobalEntity, E>,
}

impl<E: Copy + Eq + Hash + Send + Sync> GlobalWorldManager<E> {
    pub fn new() -> Self {
        Self {
            diff_handler: Arc::new(RwLock::new(GlobalDiffHandler::new())),
            entity_records: HashMap::default(),
            global_entity_map: BigMap::new(),
        }
    }

    // Entities
    pub fn entities(&self) -> Vec<E> {
        let mut output = Vec::new();

        for (entity, _) in &self.entity_records {
            output.push(*entity);
        }

        output
    }

    pub fn has_entity(&self, entity: &E) -> bool {
        self.entity_records.contains_key(entity)
    }

    pub fn entity_owner(&self, entity: &E) -> Option<EntityOwner> {
        if let Some(record) = self.entity_records.get(entity) {
            return Some(record.owner);
        }
        return None;
    }

    // Spawn
    pub fn host_spawn_entity(&mut self, entity: &E) {
        if self.entity_records.contains_key(entity) {
            panic!("entity already initialized!");
        }
        let global_entity = self.global_entity_map.insert(*entity);
        self.entity_records.insert(
            *entity,
            GlobalEntityRecord::new(global_entity, EntityOwner::Server),
        );
    }

    // Despawn
    pub fn host_despawn_entity(&mut self, entity: &E) -> Option<GlobalEntityRecord> {
        // Clean up associated components
        for component_kind in self.component_kinds(entity).unwrap() {
            self.host_remove_component(entity, &component_kind);
        }

        // Despawn from World Record
        if !self.entity_records.contains_key(entity) {
            panic!("entity does not exist!");
        }

        self.entity_records.remove(entity)
    }

    // Component Kinds
    pub fn component_kinds(&self, entity: &E) -> Option<Vec<ComponentKind>> {
        if !self.entity_records.contains_key(entity) {
            return None;
        }

        let component_kind_set = &self.entity_records.get(entity).unwrap().component_kinds;
        return Some(component_kind_set.iter().copied().collect());
    }

    // Insert Component
    pub fn host_insert_component(&mut self, entity: &E, component: &mut dyn Replicate) {
        let component_kind = component.kind();
        let diff_mask_length: u8 = component.diff_mask_size();

        if !self.entity_records.contains_key(entity) {
            panic!("entity does not exist!");
        }
        let component_kind_set = &mut self.entity_records.get_mut(entity).unwrap().component_kinds;
        component_kind_set.insert(component_kind);

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
    pub fn host_remove_component(&mut self, entity: &E, component_kind: &ComponentKind) {
        if !self.entity_records.contains_key(entity) {
            panic!("entity does not exist!");
        }
        let component_kind_set = &mut self.entity_records.get_mut(entity).unwrap().component_kinds;
        if !component_kind_set.remove(component_kind) {
            panic!("component does not exist!");
        }

        self.diff_handler
            .as_ref()
            .write()
            .expect("Haven't initialized DiffHandler")
            .deregister_component(entity, component_kind);
    }

    pub fn remote_spawn_entity_record(&mut self, entity: &E, user_key: &UserKey) {
        let Some(record) = self.entity_records.get_mut(entity) else {
            panic!("entity record does not exist!");
        };

        if record.owner != EntityOwner::ClientWaiting(*user_key) {
            panic!("client entity record is not waiting to be updated!");
        }

        record.owner = EntityOwner::Client(*user_key);
    }
}

impl<E: Copy + Eq + Hash + Send + Sync> GlobalWorldManagerType<E> for GlobalWorldManager<E> {
    fn component_kinds(&self, entity: &E) -> Option<Vec<ComponentKind>> {
        self.component_kinds(entity)
    }

    fn to_global_entity_converter(&self) -> &dyn EntityAndGlobalEntityConverter<E> {
        self
    }

    fn entity_can_relate_to_user(&self, entity: &E, user_key: &u64) -> bool {
        if let Some(record) = self.entity_records.get(entity) {
            return match record.owner {
                EntityOwner::Server => true,
                EntityOwner::Client(owning_user_key)
                | EntityOwner::ClientWaiting(owning_user_key) => {
                    return owning_user_key.to_u64() == *user_key;
                }
                EntityOwner::Local => false,
            };
        }
        return false;
    }

    fn new_mut_channel(&self, diff_mask_length: u8) -> Arc<RwLock<dyn MutChannelType>> {
        let mut_channel = MutChannelData::new(diff_mask_length);
        return Arc::new(RwLock::new(mut_channel));
    }

    fn diff_handler(&self) -> Arc<RwLock<GlobalDiffHandler<E>>> {
        self.diff_handler.clone()
    }

    fn remote_spawn_entity(&mut self, entity: &E, user_key: &u64) {
        if self.entity_records.contains_key(entity) {
            panic!("entity already initialized!");
        }
        let global_entity = self.global_entity_map.insert(*entity);
        self.entity_records.insert(
            *entity,
            GlobalEntityRecord::new(
                global_entity,
                EntityOwner::ClientWaiting(UserKey::from_u64(*user_key)),
            ),
        );
    }

    fn remote_despawn_entity(&mut self, entity: &E) {
        let record = self
            .entity_records
            .remove(entity)
            .expect("Cannot despawn non-existant entity!");
        let global_entity = record.global_entity;
        self.global_entity_map.remove(&global_entity);
    }
}

impl<E: Copy + Eq + Hash + Send + Sync> EntityAndGlobalEntityConverter<E>
    for GlobalWorldManager<E>
{
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<E, EntityDoesNotExistError> {
        if let Some(entity) = self.global_entity_map.get(global_entity) {
            Ok(*entity)
        } else {
            Err(EntityDoesNotExistError)
        }
    }

    fn entity_to_global_entity(&self, entity: &E) -> Result<GlobalEntity, EntityDoesNotExistError> {
        if let Some(record) = self.entity_records.get(entity) {
            Ok(record.global_entity)
        } else {
            Err(EntityDoesNotExistError)
        }
    }
}
