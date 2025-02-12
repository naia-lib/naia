use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use log::info;

use naia_shared::{
    ComponentKind, EntityAuthAccessor, EntityAuthStatus,
    GlobalDiffHandler, GlobalEntity, GlobalWorldManagerType,
    HostAuthHandler, HostType, MutChannelType, PropertyMutator, Replicate,
};

use super::global_entity_record::GlobalEntityRecord;
use crate::{
    world::{entity_owner::EntityOwner, mut_channel::MutChannelData},
    ReplicationConfig,
};

pub struct GlobalWorldManager {
    /// Manages authorization to mutate delegated Entities
    auth_handler: HostAuthHandler,
    /// Manages mutation of individual Component properties
    diff_handler: Arc<RwLock<GlobalDiffHandler>>,
    /// Information about entities in the internal ECS World
    entity_records: HashMap<GlobalEntity, GlobalEntityRecord>,
}

impl GlobalWorldManager {
    pub fn new() -> Self {
        Self {
            auth_handler: HostAuthHandler::new(),
            diff_handler: Arc::new(RwLock::new(GlobalDiffHandler::new())),
            entity_records: HashMap::default(),
        }
    }

    // Entities
    pub fn entities(&self) -> Vec<GlobalEntity> {
        let mut output = Vec::new();

        for (global_entity, _) in &self.entity_records {
            output.push(*global_entity);
        }

        output
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
    pub fn insert_entity_record(&mut self, global_entity: &GlobalEntity) {
        if self.entity_records.contains_key(global_entity) {
            panic!("entity already initialized!");
        }
        self.entity_records.insert(
            *global_entity,
            GlobalEntityRecord::new(EntityOwner::Client),
        );
    }

    // Despawn
    pub fn host_despawn_entity(&mut self, global_entity: &GlobalEntity) -> Option<GlobalEntityRecord> {
        // Clean up associated components
        for component_kind in self.component_kinds(global_entity).unwrap() {
            self.host_remove_component(global_entity, &component_kind);
        }

        // Despawn from World Record
        if !self.entity_records.contains_key(global_entity) {
            panic!("entity does not exist!");
        }

        self.entity_records.remove(global_entity)
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
    pub fn host_insert_component(&mut self, global_entity: &GlobalEntity, component: &mut dyn Replicate) {
        let component_kind = component.kind();
        let diff_mask_length: u8 = component.diff_mask_size();

        if !self.entity_records.contains_key(global_entity) {
            panic!("entity does not exist!");
        }
        let component_kind_set = &mut self.entity_records.get_mut(global_entity).unwrap().component_kinds;
        component_kind_set.insert(component_kind);

        let prop_mutator = self.register_component(global_entity, &component_kind, diff_mask_length);

        component.set_mutator(&prop_mutator);
    }

    // Remove Component
    pub fn host_remove_component(&mut self, global_entity: &GlobalEntity, component_kind: &ComponentKind) {
        if !self.entity_records.contains_key(global_entity) {
            panic!("entity does not exist!");
        }
        let component_kind_set = &mut self.entity_records.get_mut(global_entity).unwrap().component_kinds;
        if !component_kind_set.remove(component_kind) {
            panic!("component does not exist!");
        }

        self.diff_handler
            .as_ref()
            .write()
            .expect("Haven't initialized DiffHandler")
            .deregister_component(global_entity, component_kind);
    }

    pub fn remote_spawn_entity(&mut self, global_entity: &GlobalEntity) {
        if self.entity_records.contains_key(global_entity) {
            panic!("entity already initialized!");
        }
        self.entity_records.insert(
            *global_entity,
            GlobalEntityRecord::new(EntityOwner::Server),
        );
    }

    pub fn remove_entity_record(&mut self, global_entity: &GlobalEntity) {
        self
            .entity_records
            .remove(global_entity)
            .expect("Cannot despawn non-existant entity!");
    }

    pub fn remote_insert_component(&mut self, global_entity: &GlobalEntity, component_kind: &ComponentKind) {
        if !self.entity_records.contains_key(global_entity) {
            panic!("entity does not exist!");
        }
        let component_kind_set = &mut self.entity_records.get_mut(global_entity).unwrap().component_kinds;
        component_kind_set.insert(*component_kind);
    }

    pub fn remote_remove_component(&mut self, global_entity: &GlobalEntity, component_kind: &ComponentKind) {
        if !self.entity_records.contains_key(global_entity) {
            panic!("entity does not exist!");
        }
        let component_kind_set = &mut self.entity_records.get_mut(global_entity).unwrap().component_kinds;
        if !component_kind_set.remove(component_kind) {
            panic!("component does not exist!");
        }
    }

    pub(crate) fn entity_replication_config(&self, global_entity: &GlobalEntity) -> Option<ReplicationConfig> {
        if let Some(record) = self.entity_records.get(global_entity) {
            return Some(record.replication_config);
        }
        return None;
    }

    pub(crate) fn entity_publish(&mut self, global_entity: &GlobalEntity) {
        let Some(record) = self.entity_records.get_mut(global_entity) else {
            panic!("entity record does not exist!");
        };
        record.replication_config = ReplicationConfig::Public;
    }

    pub(crate) fn entity_unpublish(&mut self, global_entity: &GlobalEntity) {
        let Some(record) = self.entity_records.get_mut(global_entity) else {
            panic!("entity record does not exist!");
        };
        record.replication_config = ReplicationConfig::Public;
    }

    pub(crate) fn entity_is_delegated(&self, global_entity: &GlobalEntity) -> bool {
        if let Some(record) = self.entity_records.get(global_entity) {
            return record.replication_config == ReplicationConfig::Delegated;
        }
        return false;
    }

    pub(crate) fn entity_register_auth_for_delegation(&mut self, global_entity: &GlobalEntity) {
        let Some(record) = self.entity_records.get_mut(global_entity) else {
            panic!("entity record does not exist!");
        };
        if record.replication_config != ReplicationConfig::Public {
            panic!(
                "Can only enable delegation on an Entity that is Public! Config: {:?}",
                record.replication_config
            );
        }
        self.auth_handler.register_entity(HostType::Client, global_entity);
    }

    pub(crate) fn entity_enable_delegation(&mut self, global_entity: &GlobalEntity) {
        let Some(record) = self.entity_records.get_mut(global_entity) else {
            panic!("entity record does not exist!");
        };
        if record.replication_config != ReplicationConfig::Public {
            panic!("Can only enable delegation on an Entity that is Public!");
        }

        record.replication_config = ReplicationConfig::Delegated;

        if record.owner.is_client() {
            record.owner = EntityOwner::Server;
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
        self.auth_handler
            .auth_status(global_entity)
            .map(|host_status| host_status.status())
    }

    pub(crate) fn entity_request_authority(&mut self, global_entity: &GlobalEntity) -> bool {
        let Some(auth_status) = self.auth_handler.auth_status(global_entity) else {
            panic!("Can only request authority for an Entity that is Delegated!");
        };
        if !auth_status.can_request() {
            // Cannot request authority for an Entity that is not Available!
            return false;
        }
        self.auth_handler
            .set_auth_status(global_entity, EntityAuthStatus::Requested);
        return true;
    }

    pub(crate) fn entity_release_authority(&mut self, global_entity: &GlobalEntity) -> bool {
        let Some(auth_status) = self.auth_handler.auth_status(global_entity) else {
            panic!("Can only release authority for an Entity that is Delegated!");
        };
        if !auth_status.can_release() {
            return false;
        }
        self.auth_handler
            .set_auth_status(global_entity, EntityAuthStatus::Releasing);
        return true;
    }

    pub(crate) fn entity_update_authority(&self, global_entity: &GlobalEntity, new_auth_status: EntityAuthStatus) {
        self.auth_handler.set_auth_status(global_entity, new_auth_status);
    }
}

impl GlobalWorldManagerType for GlobalWorldManager {
    fn component_kinds(&self, global_entity: &GlobalEntity) -> Option<Vec<ComponentKind>> {
        self.component_kinds(global_entity)
    }

    fn entity_can_relate_to_user(&self, global_entity: &GlobalEntity, _user_key: &u64) -> bool {
        if let Some(record) = self.entity_records.get(global_entity) {
            return match record.owner {
                EntityOwner::Server | EntityOwner::Client => true,
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
            .register_component(self, global_entity, &component_kind, diff_mask_length);

        PropertyMutator::new(mut_sender)
    }

    fn get_entity_auth_accessor(&self, global_entity: &GlobalEntity) -> EntityAuthAccessor {
        self.auth_handler.get_accessor(global_entity)
    }

    fn entity_needs_mutator_for_delegation(&self, global_entity: &GlobalEntity) -> bool {
        if let Some(record) = self.entity_records.get(global_entity) {
            let server_owned = record.owner == EntityOwner::Server;
            let is_public = record.replication_config == ReplicationConfig::Public;

            return server_owned && is_public;
        }
        info!("entity_needs_mutator_for_delegation: entity does not have record");
        return false;
    }

    fn entity_is_replicating(&self, global_entity: &GlobalEntity) -> bool {
        let Some(record) = self.entity_records.get(global_entity) else {
            panic!("entity does not have record");
        };
        return record.is_replicating;
    }
}