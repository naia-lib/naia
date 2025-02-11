use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};

use log::warn;

use naia_shared::{
    BigMapKey, ComponentKind, EntityAuthAccessor,
    EntityAuthStatus, GlobalDiffHandler, GlobalEntity,
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

pub struct GlobalWorldManager {
    /// Manages authorization to mutate delegated Entities
    auth_handler: ServerAuthHandler,
    /// Manages mutation of individual Component properties
    diff_handler: Arc<RwLock<GlobalDiffHandler>>,
    /// Information about entities in the internal ECS World
    entity_records: HashMap<GlobalEntity, GlobalEntityRecord>,
}

impl GlobalWorldManager {
    pub fn new() -> Self {
        Self {
            auth_handler: ServerAuthHandler::new(),
            diff_handler: Arc::new(RwLock::new(GlobalDiffHandler::new())),
            entity_records: HashMap::new(),
        }
    }

    pub fn has_entity(&self, global_entity: &GlobalEntity) -> bool {
        self.entity_records.contains_key(global_entity)
    }

    pub fn entity_owner(&self, global_entity: &GlobalEntity) -> Option<EntityOwner> {
        if let Some(record) = self.entity_records.get(global_entity) {
            return Some(record.owner);
        }
        return None;
    }

    // Spawn
    pub fn insert_entity_record(&mut self, global_entity: &GlobalEntity, entity_owner: EntityOwner) {
        if self.entity_records.contains_key(global_entity) {
            panic!("entity already initialized!");
        }
        self.entity_records.insert(
            *global_entity,
            GlobalEntityRecord::new(entity_owner),
        );
    }

    // Despawn
    pub fn remove_entity_diff_handlers(&mut self, global_entity: &GlobalEntity) {
        // Clean up associated components
        for component_kind in self.component_kinds(global_entity).unwrap() {
            self.remove_component_diff_handler(global_entity, &component_kind);
        }
    }

    pub fn remove_entity_record(&mut self, global_entity: &GlobalEntity) {
        self
            .entity_records
            .remove(global_entity)
            .expect("Cannot despawn non-existant entity!");
    }

    // Component Kinds
    pub fn component_kinds(&self, global_entity: &GlobalEntity) -> Option<Vec<ComponentKind>> {
        if !self.entity_records.contains_key(global_entity) {
            return None;
        }

        let component_kind_set = &self.entity_records.get(global_entity).unwrap().component_kinds;
        return Some(component_kind_set.iter().copied().collect());
    }

    // Insert Component
    pub fn insert_component_record(&mut self, global_entity: &GlobalEntity, component_kind: &ComponentKind) {
        if !self.entity_records.contains_key(global_entity) {
            panic!("entity does not exist!");
        }
        let component_kind_set = &mut self.entity_records.get_mut(global_entity).unwrap().component_kinds;
        component_kind_set.insert(*component_kind);
    }

    pub fn has_component_record(&self, global_entity: &GlobalEntity, component_kind: &ComponentKind) -> bool {
        if !self.entity_records.contains_key(global_entity) {
            return false;
        }
        let component_kind_set = &self.entity_records.get(global_entity).unwrap().component_kinds;
        return component_kind_set.contains(component_kind);
    }

    pub fn insert_component_diff_handler(&mut self, global_entity: &GlobalEntity, component: &mut dyn Replicate) {
        let kind = component.kind();
        let diff_mask_length: u8 = component.diff_mask_size();
        let prop_mutator = self.register_component(global_entity, &kind, diff_mask_length);
        component.set_mutator(&prop_mutator);
    }

    // Remove Component
    pub fn remove_component_record(&mut self, global_entity: &GlobalEntity, component_kind: &ComponentKind) {
        if !self.entity_records.contains_key(global_entity) {
            panic!("entity does not exist!");
        }
        let component_kind_set = &mut self.entity_records.get_mut(global_entity).unwrap().component_kinds;
        if !component_kind_set.remove(component_kind) {
            panic!("component does not exist!");
        }
    }

    pub fn remove_component_diff_handler(&mut self, global_entity: &GlobalEntity, component_kind: &ComponentKind) {
        self.diff_handler
            .as_ref()
            .write()
            .expect("Haven't initialized DiffHandler")
            .deregister_component(global_entity, component_kind);
    }

    // Public

    pub(crate) fn entity_publish(&mut self, global_entity: &GlobalEntity) -> bool {
        let Some(record) = self.entity_records.get_mut(global_entity) else {
            panic!("entity record does not exist!");
        };

        match record.owner {
            EntityOwner::Local => {
                panic!(
                    "Can only publish an Entity that is owned by a Client! Current owner: {:?}",
                    record.owner
                );
            }
            EntityOwner::Server => {
                warn!(
                    "Can only publish an Entity that is owned by a Client! Current owner: {:?}",
                    record.owner
                );
                return false;
            }
            EntityOwner::ClientWaiting(_user_key) => {
                panic!("Attempting to publish an Entity that is waiting for a Client to take ownership");
            }
            EntityOwner::Client(user_key) => {
                // info!("Publishing Entity owned by User: {:?}", user_key);
                record.owner = EntityOwner::ClientPublic(user_key);
                record.replication_config = ReplicationConfig::Public;
                return true;
            }
            EntityOwner::ClientPublic(user_key) => {
                warn!("Published Entity is being published again!");
                record.owner = EntityOwner::ClientPublic(user_key);
                record.replication_config = ReplicationConfig::Public;
                return true;
            }
        }
    }

    pub(crate) fn entity_unpublish(&mut self, global_entity: &GlobalEntity) {
        let Some(record) = self.entity_records.get_mut(global_entity) else {
            panic!("entity record does not exist!");
        };
        if let EntityOwner::ClientPublic(user_key) = record.owner {
            record.owner = EntityOwner::Client(user_key);
            record.replication_config = ReplicationConfig::Private;
        } else {
            panic!("Can only unpublish an Entity that is Client-owned and Public!");
        }
    }

    pub(crate) fn entity_is_public_and_client_owned(&self, global_entity: &GlobalEntity) -> bool {
        let Some(record) = self.entity_records.get(global_entity) else {
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
        global_entity: &GlobalEntity,
    ) -> bool {
        let Some(record) = self.entity_records.get(global_entity) else {
            panic!("entity record does not exist!");
        };
        match &record.owner {
            EntityOwner::ClientPublic(owning_user_key) => owning_user_key == user_key,
            _ => false,
        }
    }

    pub(crate) fn entity_replication_config(&self, global_entity: &GlobalEntity) -> Option<ReplicationConfig> {
        if let Some(record) = self.entity_records.get(global_entity) {
            return Some(record.replication_config);
        }
        return None;
    }

    pub(crate) fn entity_is_delegated(&self, global_entity: &GlobalEntity) -> bool {
        if let Some(record) = self.entity_records.get(global_entity) {
            return record.replication_config == ReplicationConfig::Delegated;
        }
        return false;
    }

    pub(crate) fn entity_enable_delegation(&mut self, global_entity: &GlobalEntity) {
        let Some(record) = self.entity_records.get_mut(global_entity) else {
            panic!("entity record does not exist!");
        };
        if record.replication_config != ReplicationConfig::Public {
            panic!("Can only enable delegation on an Entity that is Public!");
        }

        record.replication_config = ReplicationConfig::Delegated;
        self.auth_handler.register_entity(global_entity);
    }

    pub(crate) fn migrate_entity_to_server(&mut self, global_entity: &GlobalEntity) {
        let Some(record) = self.entity_records.get_mut(global_entity) else {
            panic!("entity record does not exist!");
        };

        if record.owner.is_client() {
            record.owner = EntityOwner::Server;
        } else {
            panic!("Can only migrate an Entity that is owned by a Client!");
        }
    }

    pub(crate) fn entity_disable_delegation(&mut self, global_entity: &GlobalEntity) {
        let Some(record) = self.entity_records.get_mut(global_entity) else {
            panic!("entity record does not exist!");
        };
        if record.replication_config != ReplicationConfig::Delegated {
            panic!("Can only disable delegation on an Entity that is Delegated!");
        }

        record.replication_config = ReplicationConfig::Public;
        self.auth_handler.deregister_entity(global_entity);
    }

    pub(crate) fn entity_authority_status(&self, global_entity: &GlobalEntity) -> Option<EntityAuthStatus> {
        self.auth_handler.authority_status(global_entity)
    }

    // returns whether or not any change to auth needed to be made
    pub(crate) fn server_take_authority(&mut self, global_entity: &GlobalEntity) -> bool {
        self.auth_handler.server_take_authority(global_entity)
    }

    pub(crate) fn client_request_authority(&mut self, global_entity: &GlobalEntity, requester: &AuthOwner) -> bool {
        self.auth_handler
            .client_request_authority(global_entity, requester)
    }

    pub(crate) fn client_release_authority(&mut self, global_entity: &GlobalEntity, releaser: &AuthOwner) -> bool {
        self.auth_handler.client_release_authority(global_entity, releaser)
    }

    pub(crate) fn user_all_owned_entities(&self, user_key: &UserKey) -> Option<&HashSet<GlobalEntity>> {
        self.auth_handler.user_all_owned_entities(user_key)
    }

    pub(crate) fn pause_entity_replication(&mut self, global_entity: &GlobalEntity) {
        let Some(record) = self.entity_records.get_mut(global_entity) else {
            panic!("entity record does not exist!");
        };
        record.is_replicating = false;
    }

    pub(crate) fn resume_entity_replication(&mut self, global_entity: &GlobalEntity) {
        let Some(record) = self.entity_records.get_mut(global_entity) else {
            panic!("entity record does not exist!");
        };
        record.is_replicating = true;
    }
}

impl GlobalWorldManagerType for GlobalWorldManager {
    fn component_kinds(&self, global_entity: &GlobalEntity) -> Option<Vec<ComponentKind>> {
        self.component_kinds(global_entity)
    }

    /// Whether or not a given user can receive a Message/Component with an EntityProperty relating to the given Entity
    fn entity_can_relate_to_user(&self, global_entity: &GlobalEntity, user_key: &u64) -> bool {
        if let Some(record) = self.entity_records.get(global_entity) {
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

    fn diff_handler(&self) -> Arc<RwLock<GlobalDiffHandler>> {
        self.diff_handler.clone()
    }

    fn register_component(
        &self,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
        diff_mask_length: u8,
    ) -> PropertyMutator {
        let mut_sender = self
            .diff_handler
            .as_ref()
            .write()
            .expect("DiffHandler should be initialized")
            .register_component(self, global_entity, component_kind, diff_mask_length);

        PropertyMutator::new(mut_sender)
    }

    fn get_entity_auth_accessor(&self, global_entity: &GlobalEntity) -> EntityAuthAccessor {
        self.auth_handler.get_accessor(global_entity)
    }

    fn entity_needs_mutator_for_delegation(&self, _global_entity: &GlobalEntity) -> bool {
        return false;
    }

    fn entity_is_replicating(&self, global_entity: &GlobalEntity) -> bool {
        let Some(record) = self.entity_records.get(global_entity) else {
            panic!("entity record does not exist!");
        };
        return record.is_replicating;
    }
}
