use std::{
    hash::Hash,
    sync::{Arc, RwLock},
};

use crate::world::local::local_entity::{HostEntity, OwnedLocalEntity, RemoteEntity};
use crate::world::update::mut_channel::MutChannelType;
use crate::{
    bigmap::BigMapKey,
    world::{
        delegation::auth_channel::EntityAuthAccessor,
        entity::{error::EntityDoesNotExistError, global_entity::GlobalEntity},
    },
    ComponentKind, ComponentKinds, GlobalDiffHandler, HostEntityGenerator, InScopeEntities,
    LocalEntityMap, PropertyMutator,
};

pub trait GlobalWorldManagerType: InScopeEntities<GlobalEntity> {
    fn component_kinds(&self, entity: &GlobalEntity) -> Option<Vec<ComponentKind>>;
    /// Whether or not a given user can receive a Message/Component with an EntityProperty relating to the given Entity
    fn entity_can_relate_to_user(&self, global_entity: &GlobalEntity, user_key: &u64) -> bool;
    fn new_mut_channel(&self, diff_mask_length: u8) -> Arc<RwLock<dyn MutChannelType>>;
    fn diff_handler(&self) -> Arc<RwLock<GlobalDiffHandler>>;
    fn register_component(
        &self,
        component_kinds: &ComponentKinds,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
        diff_mask_length: u8,
    ) -> PropertyMutator;
    fn get_entity_auth_accessor(&self, global_entity: &GlobalEntity) -> EntityAuthAccessor;
    fn entity_needs_mutator_for_delegation(&self, global_entity: &GlobalEntity) -> bool;
    fn entity_is_replicating(&self, global_entity: &GlobalEntity) -> bool;
    fn entity_is_static(&self, global_entity: &GlobalEntity) -> bool;
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
    fn owned_entity_to_global_entity(
        &self,
        owned_entity: &OwnedLocalEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        match owned_entity {
            OwnedLocalEntity::Host { id: host_entity, .. } => {
                self.host_entity_to_global_entity(&HostEntity::new(*host_entity))
            }
            OwnedLocalEntity::Remote(remote_entity) => {
                self.remote_entity_to_global_entity(&RemoteEntity::new(*remote_entity))
            }
        }
    }
    fn apply_entity_redirect(&self, entity: &OwnedLocalEntity) -> OwnedLocalEntity;
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
        Ok(OwnedLocalEntity::Host { id: 0, is_static: false })
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

    fn apply_entity_redirect(&self, entity: &OwnedLocalEntity) -> OwnedLocalEntity {
        *entity // No redirects in fake converter
    }
}

impl LocalEntityAndGlobalEntityConverterMut for FakeEntityConverter {
    fn get_or_reserve_entity(
        &mut self,
        _global_entity: &GlobalEntity,
    ) -> Result<OwnedLocalEntity, EntityDoesNotExistError> {
        Ok(OwnedLocalEntity::Host { id: 0, is_static: false })
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
    local_entity_map: &'b mut LocalEntityMap,
    host_entity_generator: &'b mut HostEntityGenerator,
}

impl<'a, 'b> EntityConverterMut<'a, 'b> {
    pub fn new(
        global_world_manager: &'a dyn GlobalWorldManagerType,
        local_entity_map: &'b mut LocalEntityMap,
        host_entity_generator: &'b mut HostEntityGenerator,
    ) -> Self {
        Self {
            global_world_manager,
            local_entity_map,
            host_entity_generator,
        }
    }
}

impl<'a, 'b> LocalEntityAndGlobalEntityConverter for EntityConverterMut<'a, 'b> {
    fn global_entity_to_host_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<HostEntity, EntityDoesNotExistError> {
        self.local_entity_map
            .entity_converter()
            .global_entity_to_host_entity(global_entity)
    }

    fn global_entity_to_remote_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<RemoteEntity, EntityDoesNotExistError> {
        self.local_entity_map
            .entity_converter()
            .global_entity_to_remote_entity(global_entity)
    }

    fn global_entity_to_owned_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<OwnedLocalEntity, EntityDoesNotExistError> {
        self.local_entity_map
            .entity_converter()
            .global_entity_to_owned_entity(global_entity)
    }

    fn host_entity_to_global_entity(
        &self,
        host_entity: &HostEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.local_entity_map
            .entity_converter()
            .host_entity_to_global_entity(host_entity)
    }

    fn remote_entity_to_global_entity(
        &self,
        remote_entity: &RemoteEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.local_entity_map
            .entity_converter()
            .remote_entity_to_global_entity(remote_entity)
    }

    fn apply_entity_redirect(&self, entity: &OwnedLocalEntity) -> OwnedLocalEntity {
        self.local_entity_map
            .entity_converter()
            .apply_entity_redirect(entity)
    }
}

impl<'a, 'b> LocalEntityAndGlobalEntityConverterMut for EntityConverterMut<'a, 'b> {
    fn get_or_reserve_entity(
        &mut self,
        global_entity: &GlobalEntity,
    ) -> Result<OwnedLocalEntity, EntityDoesNotExistError> {
        if !self
            .global_world_manager
            .entity_can_relate_to_user(global_entity, self.host_entity_generator.get_user_key())
        {
            return Err(EntityDoesNotExistError);
        }
        let result = self
            .local_entity_map
            .global_entity_to_owned_entity(global_entity);
        if result.is_ok() {
            // info!("get_or_reserve_entity(). `global_entity`: {:?} --> `owned_entity`: {:?}", global_entity, result);
            return result;
        }

        let host_entity = self
            .host_entity_generator
            .host_reserve_entity(self.local_entity_map, global_entity);

        // warn!("get_or_reserve_entity() `global_entity` {:?} is not owned by user, attempting to reserve. `host_entity`: {:?}", global_entity, host_entity);

        return Ok(host_entity.copy_to_owned());
    }
}
