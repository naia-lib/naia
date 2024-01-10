use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    sync::{Arc, RwLock},
};

use naia_shared::{
    BigMap, BigMapKey, ComponentKind, EntityAndGlobalEntityConverter, EntityAuthAccessor,
    EntityAuthStatus, EntityDoesNotExistError, GlobalDiffHandler, GlobalEntity,
    GlobalWorldManagerType, MutChannelType, PropertyMutator, Replicate,
};

use super::global_entity_record::GlobalEntityRecord;
use crate::{
    world::{
        mut_channel::MutChannelData,
        server_auth_handler::{AuthOwner, ServerAuthHandler},
    },
    EntityOwner, ReplicationConfig, UserKey,
};

pub struct GlobalWorldManager<E: Copy + Eq + Hash + Send + Sync> {
    /// Manages authorization to mutate delegated Entities
    auth_handler: ServerAuthHandler<E>,
    /// Manages mutation of individual Component properties
    diff_handler: Arc<RwLock<GlobalDiffHandler<E>>>,
    /// Information about entities in the internal ECS World
    entity_records: HashMap<E, GlobalEntityRecord>,
    /// Map from the internal [`GlobalEntity`] to the external (e.g. Bevy's) entity id
    global_entity_map: BigMap<GlobalEntity, E>,
}

impl<E: Copy + Eq + Hash + Send + Sync> GlobalWorldManager<E> {
    pub fn new() -> Self {
        Self {
            auth_handler: ServerAuthHandler::new(),
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
    pub fn spawn_entity_record(&mut self, entity: &E, entity_owner: EntityOwner) {
        if self.entity_records.contains_key(entity) {
            panic!("entity already initialized!");
        }
        let global_entity = self.global_entity_map.insert(*entity);
        self.entity_records.insert(
            *entity,
            GlobalEntityRecord::new(global_entity, entity_owner),
        );
    }

    // Despawn
    pub fn remove_entity_diff_handlers(&mut self, entity: &E) {
        // Clean up associated components
        for component_kind in self.component_kinds(entity).unwrap() {
            self.remove_component_diff_handler(entity, &component_kind);
        }
    }

    pub fn remove_entity_record(&mut self, entity: &E) {
        let record = self
            .entity_records
            .remove(entity)
            .expect("Cannot despawn non-existant entity!");
        let global_entity = record.global_entity;
        self.global_entity_map.remove(&global_entity);
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
    pub fn insert_component_record(&mut self, entity: &E, component_kind: &ComponentKind) {
        if !self.entity_records.contains_key(entity) {
            panic!("entity does not exist!");
        }
        let component_kind_set = &mut self.entity_records.get_mut(entity).unwrap().component_kinds;
        component_kind_set.insert(*component_kind);
    }

    pub fn has_component_record(&self, entity: &E, component_kind: &ComponentKind) -> bool {
        if !self.entity_records.contains_key(entity) {
            return false;
        }
        let component_kind_set = &self.entity_records.get(entity).unwrap().component_kinds;
        return component_kind_set.contains(component_kind);
    }

    pub fn insert_component_diff_handler(&mut self, entity: &E, component: &mut dyn Replicate) {
        let kind = component.kind();
        let diff_mask_length: u8 = component.diff_mask_size();
        let prop_mutator = self.register_component(entity, &kind, diff_mask_length);
        component.set_mutator(&prop_mutator);
    }

    // Remove Component
    pub fn remove_component_record(&mut self, entity: &E, component_kind: &ComponentKind) {
        if !self.entity_records.contains_key(entity) {
            panic!("entity does not exist!");
        }
        let component_kind_set = &mut self.entity_records.get_mut(entity).unwrap().component_kinds;
        if !component_kind_set.remove(component_kind) {
            panic!("component does not exist!");
        }
    }

    pub fn remove_component_diff_handler(&mut self, entity: &E, component_kind: &ComponentKind) {
        self.diff_handler
            .as_ref()
            .write()
            .expect("Haven't initialized DiffHandler")
            .deregister_component(entity, component_kind);
    }

    // Public

    pub(crate) fn entity_publish(&mut self, entity: &E) {
        let Some(record) = self.entity_records.get_mut(entity) else {
            panic!("entity record does not exist!");
        };
        if let EntityOwner::Client(user_key) = record.owner {
            record.owner = EntityOwner::ClientPublic(user_key);
            record.replication_config = ReplicationConfig::Public;
        } else {
            panic!("Can only publish an Entity that is owned by a Client!");
        }
    }

    pub(crate) fn entity_unpublish(&mut self, entity: &E) {
        let Some(record) = self.entity_records.get_mut(entity) else {
            panic!("entity record does not exist!");
        };
        if let EntityOwner::ClientPublic(user_key) = record.owner {
            record.owner = EntityOwner::Client(user_key);
            record.replication_config = ReplicationConfig::Private;
        } else {
            panic!("Can only unpublish an Entity that is Client-owned and Public!");
        }
    }

    pub(crate) fn entity_is_public_and_client_owned(&self, entity: &E) -> bool {
        let Some(record) = self.entity_records.get(entity) else {
            panic!("entity record does not exist!");
        };
        match record.owner {
            EntityOwner::ClientPublic(_) => true,
            _ => false,
        }
    }

    pub(crate) fn entity_is_public_and_owned_by_user(
        &self,
        user_key: &UserKey,
        entity: &E,
    ) -> bool {
        let Some(record) = self.entity_records.get(entity) else {
            panic!("entity record does not exist!");
        };
        match &record.owner {
            EntityOwner::ClientPublic(owning_user_key) => owning_user_key == user_key,
            _ => false,
        }
    }

    pub(crate) fn entity_replication_config(&self, entity: &E) -> Option<ReplicationConfig> {
        if let Some(record) = self.entity_records.get(entity) {
            return Some(record.replication_config);
        }
        return None;
    }

    pub(crate) fn entity_is_delegated(&self, entity: &E) -> bool {
        if let Some(record) = self.entity_records.get(entity) {
            return record.replication_config == ReplicationConfig::Delegated;
        }
        return false;
    }

    pub(crate) fn entity_enable_delegation(&mut self, entity: &E) {
        let Some(record) = self.entity_records.get_mut(entity) else {
            panic!("entity record does not exist!");
        };
        if record.replication_config != ReplicationConfig::Public {
            panic!("Can only enable delegation on an Entity that is Public!");
        }

        record.replication_config = ReplicationConfig::Delegated;
        self.auth_handler.register_entity(entity);
    }

    pub(crate) fn migrate_entity_to_server(&mut self, entity: &E) {
        let Some(record) = self.entity_records.get_mut(entity) else {
            panic!("entity record does not exist!");
        };

        if record.owner.is_client() {
            record.owner = EntityOwner::Server;
        } else {
            panic!("Can only migrate an Entity that is owned by a Client!");
        }
    }

    pub(crate) fn entity_disable_delegation(&mut self, entity: &E) {
        let Some(record) = self.entity_records.get_mut(entity) else {
            panic!("entity record does not exist!");
        };
        if record.replication_config != ReplicationConfig::Delegated {
            panic!("Can only disable delegation on an Entity that is Delegated!");
        }

        record.replication_config = ReplicationConfig::Public;
        self.auth_handler.deregister_entity(entity);
    }

    pub(crate) fn entity_authority_status(&self, entity: &E) -> Option<EntityAuthStatus> {
        self.auth_handler.authority_status(entity)
    }

    // returns whether or not any change to auth needed to be made
    pub(crate) fn server_take_authority(&mut self, entity: &E) -> bool {
        self.auth_handler.server_take_authority(entity)
    }

    pub(crate) fn client_request_authority(&mut self, entity: &E, requester: &AuthOwner) -> bool {
        self.auth_handler
            .client_request_authority(entity, requester)
    }

    pub(crate) fn client_release_authority(&mut self, entity: &E, releaser: &AuthOwner) -> bool {
        self.auth_handler.client_release_authority(entity, releaser)
    }

    pub(crate) fn user_all_owned_entities(&self, user_key: &UserKey) -> Option<&HashSet<E>> {
        self.auth_handler.user_all_owned_entities(user_key)
    }

    pub(crate) fn pause_entity_replication(&mut self, entity: &E) {
        let Some(record) = self.entity_records.get_mut(entity) else {
            panic!("entity record does not exist!");
        };
        record.is_replicating = false;
    }

    pub(crate) fn resume_entity_replication(&mut self, entity: &E) {
        let Some(record) = self.entity_records.get_mut(entity) else {
            panic!("entity record does not exist!");
        };
        record.is_replicating = true;
    }
}

impl<E: Copy + Eq + Hash + Send + Sync> GlobalWorldManagerType<E> for GlobalWorldManager<E> {
    fn component_kinds(&self, entity: &E) -> Option<Vec<ComponentKind>> {
        self.component_kinds(entity)
    }

    fn to_global_entity_converter(&self) -> &dyn EntityAndGlobalEntityConverter<E> {
        self
    }

    /// Whether or not a given user can receive a Message/Component with an EntityProperty relating to the given Entity
    fn entity_can_relate_to_user(&self, entity: &E, user_key: &u64) -> bool {
        if let Some(record) = self.entity_records.get(entity) {
            return match record.owner {
                EntityOwner::Server | EntityOwner::ClientPublic(_) => true,
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

    fn register_component(
        &self,
        entity: &E,
        component_kind: &ComponentKind,
        diff_mask_length: u8,
    ) -> PropertyMutator {
        let mut_sender = self
            .diff_handler
            .as_ref()
            .write()
            .expect("DiffHandler should be initialized")
            .register_component(self, entity, component_kind, diff_mask_length);

        PropertyMutator::new(mut_sender)
    }

    fn get_entity_auth_accessor(&self, entity: &E) -> EntityAuthAccessor {
        self.auth_handler.get_accessor(entity)
    }

    fn entity_needs_mutator_for_delegation(&self, _entity: &E) -> bool {
        return false;
    }

    fn entity_is_replicating(&self, entity: &E) -> bool {
        let Some(record) = self.entity_records.get(entity) else {
            panic!("entity record does not exist!");
        };
        return record.is_replicating;
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
