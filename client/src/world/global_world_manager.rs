use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use log::info;

use naia_shared::{
    AuthorityError, ComponentKind, ComponentKinds, EntityAuthAccessor, EntityAuthStatus,
    GlobalDiffHandler, GlobalEntity, GlobalWorldManagerType, HostAuthHandler, HostType,
    InScopeEntities, MutChannelType, PropertyMutator, Replicate,
};

use super::global_entity_record::GlobalEntityRecord;
use crate::{
    world::{entity_owner::EntityOwner, mut_channel::MutChannelData},
    Publicity,
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

        for global_entity in self.entity_records.keys() {
            output.push(*global_entity);
        }

        output
    }

    pub fn has_entity(&self, global_entity: &GlobalEntity) -> bool {
        self.entity_records.contains_key(global_entity)
    }

    pub fn entity_is_static(&self, global_entity: &GlobalEntity) -> bool {
        self.entity_records
            .get(global_entity)
            .map(|r| r.is_static)
            .unwrap_or(false)
    }

    pub fn entity_owner(&self, global_entity: &GlobalEntity) -> Option<EntityOwner> {
        if let Some(record) = self.entity_records.get(global_entity) {
            return Some(record.owner());
        }
        None
    }

    // Spawn
    pub fn host_spawn_entity(&mut self, global_entity: &GlobalEntity) {
        if self.entity_records.contains_key(global_entity) {
            panic!("entity already initialized!");
        }
        self.entity_records
            .insert(*global_entity, GlobalEntityRecord::new(EntityOwner::Client));
    }

    pub fn host_spawn_static_entity(&mut self, global_entity: &GlobalEntity) {
        if self.entity_records.contains_key(global_entity) {
            panic!("entity already initialized!");
        }
        self.entity_records
            .insert(*global_entity, GlobalEntityRecord::new_static(EntityOwner::Client));
    }

    pub fn mark_entity_as_static(&mut self, global_entity: &GlobalEntity) {
        let Some(record) = self.entity_records.get_mut(global_entity) else {
            panic!("entity record does not exist!");
        };
        record.is_static = true;
    }

    // Despawn
    pub fn host_despawn_entity(
        &mut self,
        global_entity: &GlobalEntity,
    ) -> Option<GlobalEntityRecord> {
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

        let component_kind_set = self
            .entity_records
            .get(global_entity)
            .unwrap()
            .component_kinds();
        Some(component_kind_set.iter().copied().collect())
    }

    // Insert Component
    pub fn host_insert_component(
        &mut self,
        component_kinds: &ComponentKinds,
        global_entity: &GlobalEntity,
        component: &mut dyn Replicate,
    ) {
        let component_kind = component.kind();
        let diff_mask_length: u8 = component.diff_mask_size();

        if !self.entity_records.contains_key(global_entity) {
            panic!("entity does not exist!");
        }
        self.entity_records
            .get_mut(global_entity)
            .unwrap()
            .insert_component(component_kind);

        let prop_mutator = self.register_component(
            component_kinds,
            global_entity,
            &component_kind,
            diff_mask_length,
        );

        component.set_mutator(&prop_mutator);
    }

    /// Returns true if this component was already registered for host-side
    /// tracking (i.e. the entity is delegated and the GlobalDiffHandler
    /// already has this entity+component registered).  Used by callers to
    /// skip redundant setup when authority is granted to a client for an
    /// entity whose delegation was already enabled.
    pub fn component_already_host_registered(
        &self,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) -> bool {
        self.entity_is_delegated(global_entity)
            && self
                .diff_handler
                .read()
                .expect("GlobalDiffHandler lock")
                .has_component(global_entity, component_kind)
    }

    // Remove Component
    pub fn host_remove_component(
        &mut self,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) {
        if !self.entity_records.contains_key(global_entity) {
            panic!("entity does not exist!");
        }
        self.entity_records
            .get_mut(global_entity)
            .unwrap()
            .remove_component(component_kind);

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
        // info!("Remote spawning entity record for {:?}", global_entity);
        self.entity_records
            .insert(*global_entity, GlobalEntityRecord::new(EntityOwner::Server));
    }

    pub fn remove_entity_record(&mut self, global_entity: &GlobalEntity) {
        self.entity_records
            .remove(global_entity)
            .expect("Cannot despawn non-existant entity!");
    }

    pub fn remote_insert_component(
        &mut self,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) {
        // info!("Remote inserting component {:?} for {:?}", component_kind, global_entity);

        if !self.entity_records.contains_key(global_entity) {
            panic!("entity does not exist!");
        }
        self.entity_records
            .get_mut(global_entity)
            .unwrap()
            .insert_component(*component_kind);
    }

    pub fn remove_component_record(
        &mut self,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) {
        if !self.entity_records.contains_key(global_entity) {
            panic!("entity does not exist!");
        }
        self.entity_records
            .get_mut(global_entity)
            .unwrap()
            .remove_component(component_kind);
    }

    pub(crate) fn entity_replication_config(
        &self,
        global_entity: &GlobalEntity,
    ) -> Option<Publicity> {
        if let Some(record) = self.entity_records.get(global_entity) {
            return Some(record.replication_config());
        }
        None
    }

    pub(crate) fn entity_publish(&mut self, global_entity: &GlobalEntity) {
        let Some(record) = self.entity_records.get_mut(global_entity) else {
            panic!("entity record does not exist!");
        };
        record.set_replication_config(Publicity::Public);
    }

    pub(crate) fn entity_unpublish(&mut self, global_entity: &GlobalEntity) {
        let Some(record) = self.entity_records.get_mut(global_entity) else {
            panic!("entity record does not exist!");
        };
        record.set_replication_config(Publicity::Private);
    }

    pub(crate) fn entity_has_component(
        &self,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) -> bool {
        if let Some(record) = self.entity_records.get(global_entity) {
            return record.has_component(component_kind);
        }
        false
    }

    pub(crate) fn entity_is_delegated(&self, global_entity: &GlobalEntity) -> bool {
        if let Some(record) = self.entity_records.get(global_entity) {
            return record.replication_config() == Publicity::Delegated;
        }
        false
    }

    pub(crate) fn entity_register_auth_for_delegation(&mut self, global_entity: &GlobalEntity) {
        let Some(record) = self.entity_records.get_mut(global_entity) else {
            panic!("entity record does not exist!");
        };
        if record.replication_config() != Publicity::Public {
            panic!(
                "Can only enable delegation on an Entity that is Public! Config: {:?}",
                record.replication_config()
            );
        }
        self.auth_handler
            .register_entity(HostType::Client, global_entity);
    }

    pub(crate) fn entity_enable_delegation(&mut self, global_entity: &GlobalEntity) {
        // info!("Enabling delegation for {:?}", global_entity);

        let Some(record) = self.entity_records.get_mut(global_entity) else {
            panic!("entity record does not exist!");
        };
        if record.replication_config() != Publicity::Public {
            panic!("Can only enable delegation on an Entity that is Public!");
        }

        record.set_replication_config(Publicity::Delegated);

        if record.owner().is_client() {
            record.set_owner(EntityOwner::Server);
        }
    }

    pub(crate) fn entity_disable_delegation(&mut self, global_entity: &GlobalEntity) {
        let Some(record) = self.entity_records.get_mut(global_entity) else {
            panic!("entity record does not exist!");
        };
        if record.replication_config() != Publicity::Delegated {
            panic!("Can only disable delegation on an Entity that is Delegated!");
        }

        record.set_replication_config(Publicity::Public);
        self.auth_handler.deregister_entity(global_entity);
    }

    pub(crate) fn entity_authority_status(
        &self,
        global_entity: &GlobalEntity,
    ) -> Option<EntityAuthStatus> {
        self.auth_handler
            .auth_status(global_entity)
            .map(|host_status| host_status.status())
    }

    pub(crate) fn entity_request_authority(
        &mut self,
        global_entity: &GlobalEntity,
    ) -> Result<(), AuthorityError> {
        if !self.has_entity(global_entity) {
            // Entity is not in scope — client has no record of it at all.
            return Err(AuthorityError::NotInScope);
        }
        if !self.entity_is_delegated(global_entity) {
            return Err(AuthorityError::NotDelegated);
        }
        let Some(auth_status) = self.auth_handler.auth_status(global_entity) else {
            // Entity is delegated in our records but auth tracking hasn't been
            // initialised yet — treat as not-in-scope (transitional state).
            return Err(AuthorityError::NotInScope);
        };
        if !auth_status.can_request() {
            // Authority is not Available (e.g. already Requested or Granted).
            return Err(AuthorityError::NotAvailable);
        }
        self.auth_handler
            .set_auth_status(global_entity, EntityAuthStatus::Requested);
        Ok(())
    }

    pub(crate) fn entity_release_authority(
        &mut self,
        global_entity: &GlobalEntity,
    ) -> Result<(), AuthorityError> {
        if !self.has_entity(global_entity) {
            return Err(AuthorityError::NotInScope);
        }
        if !self.entity_is_delegated(global_entity) {
            return Err(AuthorityError::NotDelegated);
        }
        let Some(auth_status) = self.auth_handler.auth_status(global_entity) else {
            return Err(AuthorityError::NotInScope);
        };
        if !auth_status.can_release() {
            return Err(AuthorityError::NotHolder);
        }
        self.auth_handler
            .set_auth_status(global_entity, EntityAuthStatus::Releasing);
        Ok(())
    }

    pub(crate) fn entity_update_authority(
        &self,
        global_entity: &GlobalEntity,
        new_auth_status: EntityAuthStatus,
    ) {
        self.auth_handler
            .set_auth_status(global_entity, new_auth_status);
    }
}

impl GlobalWorldManagerType for GlobalWorldManager {
    fn component_kinds(&self, global_entity: &GlobalEntity) -> Option<Vec<ComponentKind>> {
        self.component_kinds(global_entity)
    }

    fn entity_can_relate_to_user(&self, global_entity: &GlobalEntity, _user_key: &u64) -> bool {
        if let Some(record) = self.entity_records.get(global_entity) {
            return match record.owner() {
                EntityOwner::Server | EntityOwner::Client => true,
                EntityOwner::Local => false,
            };
        }
        false
    }

    fn new_mut_channel(&self, diff_mask_length: u8) -> Arc<RwLock<dyn MutChannelType>> {
        let mut_channel = MutChannelData::new(diff_mask_length);
        Arc::new(RwLock::new(mut_channel))
    }

    fn diff_handler(&self) -> Arc<RwLock<GlobalDiffHandler>> {
        self.diff_handler.clone()
    }

    fn register_component(
        &self,
        component_kinds: &ComponentKinds,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
        diff_mask_length: u8,
    ) -> PropertyMutator {
        let mut_sender = self
            .diff_handler
            .as_ref()
            .write()
            .expect("DiffHandler should be initialized")
            .register_component(
                component_kinds,
                self,
                global_entity,
                component_kind,
                diff_mask_length,
            );

        PropertyMutator::new(mut_sender)
    }

    fn get_entity_auth_accessor(&self, global_entity: &GlobalEntity) -> EntityAuthAccessor {
        self.auth_handler.get_accessor(global_entity)
    }

    fn entity_needs_mutator_for_delegation(&self, global_entity: &GlobalEntity) -> bool {
        if let Some(record) = self.entity_records.get(global_entity) {
            let server_owned = record.owner() == EntityOwner::Server;
            let is_public = record.replication_config() == Publicity::Public;

            return server_owned && is_public;
        }
        info!("entity_needs_mutator_for_delegation: entity does not have record");
        false
    }

    fn entity_is_replicating(&self, global_entity: &GlobalEntity) -> bool {
        let Some(record) = self.entity_records.get(global_entity) else {
            panic!("entity does not have record");
        };
        record.is_replicating()
    }

    fn entity_is_static(&self, global_entity: &GlobalEntity) -> bool {
        Self::entity_is_static(self, global_entity)
    }
}

impl InScopeEntities<GlobalEntity> for GlobalWorldManager {
    fn has_entity(&self, global_entity: &GlobalEntity) -> bool {
        self.entity_records.contains_key(global_entity)
    }
}
