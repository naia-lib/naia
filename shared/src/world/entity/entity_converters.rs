use std::{
    hash::Hash,
    sync::{Arc, RwLock},
};

use log::warn;

use crate::{
    bigmap::BigMapKey,
    world::{
        delegation::auth_channel::EntityAuthAccessor,
        entity::{
            error::EntityDoesNotExistError,
            global_entity::GlobalEntity,
            local_entity::{HostEntity, OwnedLocalEntity, RemoteEntity},
        },
        host::mut_channel::MutChannelType,
    },
    ComponentKind, GlobalDiffHandler, LocalWorldManager, PropertyMutator,
};

pub trait GlobalWorldManagerType {
    fn component_kinds(&self, entity: &GlobalEntity) -> Option<Vec<ComponentKind>>;
    /// Whether or not a given user can receive a Message/Component with an EntityProperty relating to the given Entity
    fn entity_can_relate_to_user(&self, global_entity: &GlobalEntity, user_key: &u64) -> bool;
    fn new_mut_channel(&self, diff_mask_length: u8) -> Arc<RwLock<dyn MutChannelType>>;
    fn diff_handler(&self) -> Arc<RwLock<GlobalDiffHandler>>;
    fn register_component(
        &self,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
        diff_mask_length: u8,
    ) -> PropertyMutator;
    fn get_entity_auth_accessor(&self, global_entity: &GlobalEntity) -> EntityAuthAccessor;
    fn entity_needs_mutator_for_delegation(&self, global_entity: &GlobalEntity) -> bool;
    fn entity_is_replicating(&self, global_entity: &GlobalEntity) -> bool;
}

pub trait EntityAndGlobalEntityConverter<E: Copy + Eq + Hash + Sync + Send> {
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<E, EntityDoesNotExistError>;
    fn entity_to_global_entity(&self, entity: &E) -> Result<GlobalEntity, EntityDoesNotExistError>;
}

pub trait LocalEntityAndGlobalEntityConverter {
    fn global_entity_to_host_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<HostEntity, EntityDoesNotExistError>;
    fn global_entity_to_remote_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<RemoteEntity, EntityDoesNotExistError>;
    fn global_entity_to_owned_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<OwnedLocalEntity, EntityDoesNotExistError>;
    fn host_entity_to_global_entity(
        &self,
        host_entity: &HostEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError>;
    fn remote_entity_to_global_entity(
        &self,
        remote_entity: &RemoteEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError>;
}

pub struct FakeEntityConverter;

impl LocalEntityAndGlobalEntityConverter for FakeEntityConverter {
    fn global_entity_to_host_entity(
        &self,
        _: &GlobalEntity,
    ) -> Result<HostEntity, EntityDoesNotExistError> {
        Ok(HostEntity::new(0))
    }

    fn global_entity_to_remote_entity(
        &self,
        _: &GlobalEntity,
    ) -> Result<RemoteEntity, EntityDoesNotExistError> {
        Ok(RemoteEntity::new(0))
    }

    fn global_entity_to_owned_entity(
        &self,
        _global_entity: &GlobalEntity,
    ) -> Result<OwnedLocalEntity, EntityDoesNotExistError> {
        Ok(OwnedLocalEntity::Host(0))
    }

    fn host_entity_to_global_entity(
        &self,
        _: &HostEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        Ok(GlobalEntity::from_u64(0))
    }

    fn remote_entity_to_global_entity(
        &self,
        _: &RemoteEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        Ok(GlobalEntity::from_u64(0))
    }
}

impl LocalEntityAndGlobalEntityConverterMut for FakeEntityConverter {
    fn get_or_reserve_entity(
        &mut self,
        _global_entity: &GlobalEntity,
    ) -> Result<OwnedLocalEntity, EntityDoesNotExistError> {
        Ok(OwnedLocalEntity::Host(0))
    }
}

pub trait LocalEntityAndGlobalEntityConverterMut: LocalEntityAndGlobalEntityConverter {
    fn get_or_reserve_entity(
        &mut self,
        global_entity: &GlobalEntity,
    ) -> Result<OwnedLocalEntity, EntityDoesNotExistError>;
}

// Probably only should be used for writing messages
pub struct EntityConverterMut<'a, 'b> {
    global_world_manager: &'a dyn GlobalWorldManagerType,
    local_world_manager: &'b mut LocalWorldManager,
}

impl<'a, 'b> EntityConverterMut<'a, 'b> {
    pub fn new(
        global_world_manager: &'a dyn GlobalWorldManagerType,
        local_world_manager: &'b mut LocalWorldManager,
    ) -> Self {
        Self {
            global_world_manager,
            local_world_manager,
        }
    }
}

impl<'a, 'b> LocalEntityAndGlobalEntityConverter for EntityConverterMut<'a, 'b>
{
    fn global_entity_to_host_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<HostEntity, EntityDoesNotExistError> {
        self.local_world_manager.entity_converter().global_entity_to_host_entity(global_entity)
    }

    fn global_entity_to_remote_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<RemoteEntity, EntityDoesNotExistError> {
        self.local_world_manager
            .entity_converter()
            .global_entity_to_remote_entity(global_entity)
    }

    fn global_entity_to_owned_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<OwnedLocalEntity, EntityDoesNotExistError> {
        self.local_world_manager
            .entity_converter()
            .global_entity_to_owned_entity(global_entity)
    }

    fn host_entity_to_global_entity(
        &self,
        host_entity: &HostEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.local_world_manager
            .entity_converter()
            .host_entity_to_global_entity(host_entity)
    }

    fn remote_entity_to_global_entity(
        &self,
        remote_entity: &RemoteEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.local_world_manager
            .entity_converter()
            .remote_entity_to_global_entity(remote_entity)
    }
}

impl<'a, 'b> LocalEntityAndGlobalEntityConverterMut for EntityConverterMut<'a, 'b>
{
    fn get_or_reserve_entity(
        &mut self,
        global_entity: &GlobalEntity,
    ) -> Result<OwnedLocalEntity, EntityDoesNotExistError> {

        if !self
            .global_world_manager
            .entity_can_relate_to_user(global_entity, self.local_world_manager.get_user_key())
        {
            return Err(EntityDoesNotExistError);
        }
        let result = self.local_world_manager.entity_converter().global_entity_to_owned_entity(global_entity);
        if result.is_ok() {
            return result;
        }

        let host_entity = self.local_world_manager.host_reserve_entity(global_entity);

        warn!("get_or_reserve_entity(): entity is not owned by user, attempting to reserve. HostEntity: {:?}", host_entity);
        return Ok(host_entity.copy_to_owned());
    }
}