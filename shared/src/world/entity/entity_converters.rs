use std::{
    hash::Hash,
    sync::{Arc, RwLock},
};

use crate::world::local::local_entity::{HostEntity, OwnedLocalEntity, RemoteEntity};
use crate::world::update::global_dirty_bitset::GlobalDirtyBitset;
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

/// Global world state queries needed during message and component serialization.
pub trait GlobalWorldManagerType: InScopeEntities<GlobalEntity> {
    /// Returns the list of component kinds currently attached to `entity`, or `None` if the entity is not known.
    fn component_kinds(&self, entity: &GlobalEntity) -> Option<Vec<ComponentKind>>;
    /// Whether or not a given user can receive a Message/Component with an EntityProperty relating to the given Entity
    fn entity_can_relate_to_user(&self, global_entity: &GlobalEntity, user_key: &u64) -> bool;
    /// Creates a new `MutChannelType` of `diff_mask_length` bytes for a component's mutation tracking.
    fn new_mut_channel(&self, diff_mask_length: u8) -> Arc<RwLock<dyn MutChannelType>>;
    /// Returns a handle to the global diff handler used to fan out property mutations.
    fn diff_handler(&self) -> Arc<RwLock<GlobalDiffHandler>>;
    /// Registers a component for mutation tracking, returning a [`PropertyMutator`] wired to the global diff handler.
    fn register_component(
        &self,
        component_kinds: &ComponentKinds,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
        diff_mask_length: u8,
    ) -> PropertyMutator;
    /// Returns an [`EntityAuthAccessor`] for reading the delegation authority state of `global_entity`.
    fn get_entity_auth_accessor(&self, global_entity: &GlobalEntity) -> EntityAuthAccessor;
    /// Returns `true` if `global_entity` requires a `PropertyMutator` to notify authority changes during delegation.
    fn entity_needs_mutator_for_delegation(&self, global_entity: &GlobalEntity) -> bool;
    /// Returns `true` if `global_entity` is actively being replicated.
    fn entity_is_replicating(&self, global_entity: &GlobalEntity) -> bool;
    /// Returns `true` if `global_entity` was spawned as a static entity.
    fn entity_is_static(&self, global_entity: &GlobalEntity) -> bool;
    /// Returns the global dirty bitset for mutation tracking, or `None` on the client side.
    fn global_dirty_bitset(&self) -> Option<Arc<GlobalDirtyBitset>> {
        None
    }
}

/// Bidirectional conversion between a world-type entity `E` and a `GlobalEntity`.
pub trait EntityAndGlobalEntityConverter<E: Copy + Eq + Hash + Sync + Send> {
    /// Resolves `global_entity` to the corresponding world-local entity `E`, or returns an error if not found.
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<E, EntityDoesNotExistError>;
    /// Resolves a world-local `entity` to its stable [`GlobalEntity`] identifier, or returns an error if not found.
    fn entity_to_global_entity(&self, entity: &E) -> Result<GlobalEntity, EntityDoesNotExistError>;
}

/// Conversions between the connection-local host/remote entity representations and the global entity space.
pub trait LocalEntityAndGlobalEntityConverter {
    /// Returns the [`HostEntity`] for `global_entity` if one is registered, or an error otherwise.
    fn global_entity_to_host_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<HostEntity, EntityDoesNotExistError>;
    /// Returns the [`RemoteEntity`] for `global_entity` if one is registered, or an error otherwise.
    fn global_entity_to_remote_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<RemoteEntity, EntityDoesNotExistError>;
    /// Returns the [`OwnedLocalEntity`] (host or remote) for `global_entity`, or an error if not found.
    fn global_entity_to_owned_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<OwnedLocalEntity, EntityDoesNotExistError>;
    /// Returns the [`GlobalEntity`] for a dynamic `host_entity`, or an error if not found.
    fn host_entity_to_global_entity(
        &self,
        host_entity: &HostEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError>;
    /// Returns the [`GlobalEntity`] for a static `host_entity`, or an error if not found.
    fn static_host_entity_to_global_entity(
        &self,
        host_entity: &HostEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError>;
    /// Returns the [`GlobalEntity`] for `remote_entity`, or an error if not found.
    fn remote_entity_to_global_entity(
        &self,
        remote_entity: &RemoteEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError>;
    /// Returns the [`GlobalEntity`] for `owned_entity`, dispatching to the appropriate host or remote lookup.
    fn owned_entity_to_global_entity(
        &self,
        owned_entity: &OwnedLocalEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        match owned_entity {
            OwnedLocalEntity::Host { id, is_static: true } => {
                self.static_host_entity_to_global_entity(&HostEntity::new(*id))
            }
            OwnedLocalEntity::Host { id, is_static: false } => {
                self.host_entity_to_global_entity(&HostEntity::new(*id))
            }
            OwnedLocalEntity::Remote { id, is_static } => {
                let remote = if *is_static { RemoteEntity::new_static(*id) } else { RemoteEntity::new(*id) };
                self.remote_entity_to_global_entity(&remote)
            }
        }
    }
    /// Returns the current redirect target for `entity`, or `entity` unchanged if no redirect is installed.
    fn apply_entity_redirect(&self, entity: &OwnedLocalEntity) -> OwnedLocalEntity;
}

/// No-op converter that always succeeds with entity ID 0; useful in test contexts where real mapping is not needed.
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

    fn static_host_entity_to_global_entity(
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

/// Mutable extension of `LocalEntityAndGlobalEntityConverter` that can allocate new host-side entity slots.
pub trait LocalEntityAndGlobalEntityConverterMut: LocalEntityAndGlobalEntityConverter {
    /// Looks up the local entity for `global_entity`, reserving a new host slot if none exists yet.
    fn get_or_reserve_entity(
        &mut self,
        global_entity: &GlobalEntity,
    ) -> Result<OwnedLocalEntity, EntityDoesNotExistError>;
}

/// Stateful converter used when writing messages: looks up or reserves host-side entity slots on demand.
pub struct EntityConverterMut<'a, 'b> {
    global_world_manager: &'a dyn GlobalWorldManagerType,
    local_entity_map: &'b mut LocalEntityMap,
    host_entity_generator: &'b mut HostEntityGenerator,
}

impl<'a, 'b> EntityConverterMut<'a, 'b> {
    /// Creates an `EntityConverterMut` backed by the given world manager, entity map, and generator.
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

    fn static_host_entity_to_global_entity(
        &self,
        host_entity: &HostEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.local_entity_map
            .entity_converter()
            .static_host_entity_to_global_entity(host_entity)
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

        Ok(host_entity.copy_to_owned())
    }
}
