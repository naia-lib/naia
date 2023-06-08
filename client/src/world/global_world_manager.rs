use std::{
    collections::HashMap,
    hash::Hash,
    sync::{Arc, RwLock},
};

use log::{info, warn};

use naia_shared::{
    BigMap, ComponentKind, EntityAndGlobalEntityConverter, EntityAuthAccessor, EntityAuthStatus,
    EntityDoesNotExistError, GlobalDiffHandler, GlobalEntity, GlobalWorldManagerType,
    HostAuthHandler, HostType, MutChannelType, PropertyMutator, Replicate,
};

use super::global_entity_record::GlobalEntityRecord;
use crate::{
    world::{entity_owner::EntityOwner, mut_channel::MutChannelData},
    ReplicationConfig,
};

pub struct GlobalWorldManager<E: Copy + Eq + Hash + Send + Sync> {
    /// Manages authorization to mutate delegated Entities
    auth_handler: HostAuthHandler<E>,
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
            auth_handler: HostAuthHandler::new(),
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
            GlobalEntityRecord::new(global_entity, EntityOwner::Client),
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

    pub fn can_despawn_entity(&self, entity: &E) -> bool {
        let Some(owner) = self.entity_owner(entity) else {
            return true;
        };
        if !owner.is_server() {
            return true;
        }
        if !self.entity_is_delegated(entity) {
            return false;
        }
        return self.entity_authority_status(entity) == Some(EntityAuthStatus::Granted);
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

        let prop_mutator = self.register_component(entity, &component_kind, diff_mask_length);

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

    pub fn remote_spawn_entity(&mut self, entity: &E) {
        if self.entity_records.contains_key(entity) {
            panic!("entity already initialized!");
        }
        let global_entity = self.global_entity_map.insert(*entity);
        self.entity_records.insert(
            *entity,
            GlobalEntityRecord::new(global_entity, EntityOwner::Server),
        );
    }

    pub fn remove_entity_record(&mut self, entity: &E) {
        let record = self
            .entity_records
            .remove(entity)
            .expect("Cannot despawn non-existant entity!");
        let global_entity = record.global_entity;
        self.global_entity_map.remove(&global_entity);
    }

    pub fn remote_insert_component(&mut self, entity: &E, component_kind: &ComponentKind) {
        if !self.entity_records.contains_key(entity) {
            panic!("entity does not exist!");
        }
        let component_kind_set = &mut self.entity_records.get_mut(entity).unwrap().component_kinds;
        component_kind_set.insert(*component_kind);
    }

    pub fn remote_remove_component(&mut self, entity: &E, component_kind: &ComponentKind) {
        if !self.entity_records.contains_key(entity) {
            panic!("entity does not exist!");
        }
        let component_kind_set = &mut self.entity_records.get_mut(entity).unwrap().component_kinds;
        if !component_kind_set.remove(component_kind) {
            panic!("component does not exist!");
        }
    }

    pub(crate) fn entity_replication_config(&self, entity: &E) -> Option<ReplicationConfig> {
        if let Some(record) = self.entity_records.get(entity) {
            return Some(record.replication_config);
        }
        return None;
    }

    pub(crate) fn entity_publish(&mut self, entity: &E) {
        let Some(record) = self.entity_records.get_mut(entity) else {
            panic!("entity record does not exist!");
        };
        record.replication_config = ReplicationConfig::Public;
    }

    pub(crate) fn entity_unpublish(&mut self, entity: &E) {
        let Some(record) = self.entity_records.get_mut(entity) else {
            panic!("entity record does not exist!");
        };
        record.replication_config = ReplicationConfig::Public;
    }

    pub(crate) fn entity_is_delegated(&self, entity: &E) -> bool {
        if let Some(record) = self.entity_records.get(entity) {
            return record.replication_config == ReplicationConfig::Delegated;
        }
        return false;
    }

    pub(crate) fn entity_register_auth_for_delegation(&mut self, entity: &E) {
        let Some(record) = self.entity_records.get_mut(entity) else {
            panic!("entity record does not exist!");
        };
        if record.replication_config != ReplicationConfig::Public {
            panic!(
                "Can only enable delegation on an Entity that is Public! Config: {:?}",
                record.replication_config
            );
        }
        self.auth_handler.register_entity(HostType::Client, entity);
    }

    pub(crate) fn entity_enable_delegation(&mut self, entity: &E) {
        let Some(record) = self.entity_records.get_mut(entity) else {
            panic!("entity record does not exist!");
        };
        if record.replication_config != ReplicationConfig::Public {
            panic!("Can only enable delegation on an Entity that is Public!");
        }

        record.replication_config = ReplicationConfig::Delegated;

        if record.owner.is_client() {
            record.owner = EntityOwner::Server;

            // migrate entity's components to RemoteOwned
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
        self.auth_handler
            .auth_status(entity)
            .map(|host_status| host_status.status())
    }

    pub(crate) fn entity_request_authority(&mut self, entity: &E) -> bool {
        let Some(auth_status) = self.auth_handler.auth_status(entity)  else {
            panic!("Can only request authority for an Entity that is Delegated!");
        };
        if !auth_status.can_request() {
            // Cannot request authority for an Entity that is not Available!
            return false;
        }
        self.auth_handler
            .set_auth_status(entity, EntityAuthStatus::Requested);
        return true;
    }

    pub(crate) fn entity_release_authority(&mut self, entity: &E) -> bool {
        let Some(auth_status) = self.auth_handler.auth_status(entity) else {
            panic!("Can only releas authority for an Entity that is Delegated!");
        };
        if !auth_status.can_release() {
            return false;
        }
        self.auth_handler
            .set_auth_status(entity, EntityAuthStatus::Releasing);
        return true;
    }

    pub(crate) fn entity_update_authority(&self, entity: &E, new_auth_status: EntityAuthStatus) {
        self.auth_handler.set_auth_status(entity, new_auth_status);
    }
}

impl<E: Copy + Eq + Hash + Send + Sync> GlobalWorldManagerType<E> for GlobalWorldManager<E> {
    fn component_kinds(&self, entity: &E) -> Option<Vec<ComponentKind>> {
        self.component_kinds(entity)
    }

    fn to_global_entity_converter(&self) -> &dyn EntityAndGlobalEntityConverter<E> {
        self
    }

    fn entity_can_relate_to_user(&self, entity: &E, _user_key: &u64) -> bool {
        if let Some(record) = self.entity_records.get(entity) {
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
            .register_component(self, entity, &component_kind, diff_mask_length);

        PropertyMutator::new(mut_sender)
    }

    fn get_entity_auth_accessor(&self, entity: &E) -> EntityAuthAccessor {
        self.auth_handler.get_accessor(entity)
    }

    fn entity_needs_mutator_for_delegation(&self, entity: &E) -> bool {
        if let Some(record) = self.entity_records.get(entity) {
            let server_owned = record.owner == EntityOwner::Server;
            let is_public = record.replication_config == ReplicationConfig::Public;

            if !server_owned {
                info!("entity_needs_mutator_for_delegation: entity is not server owned");
            }
            if !is_public {
                info!("entity_needs_mutator_for_delegation: entity is not public");
            }

            return server_owned && is_public;
        }
        info!("entity_needs_mutator_for_delegation: entity does not have record");
        return false;
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
            warn!("global_world_manager failed entity_to_global_entity!");
            Err(EntityDoesNotExistError)
        }
    }
}
