use std::{
    hash::Hash,
    sync::{Arc, RwLock},
};

use crate::{
    bigmap::BigMapKey,
    world::{
        entity::{
            error::EntityDoesNotExistError, global_entity::GlobalEntity, local_entity::LocalEntity,
        },
        host::mut_channel::MutChannelType,
    },
    ComponentKind, GlobalDiffHandler, LocalWorldManager,
};

pub trait GlobalWorldManagerType<E: Copy + Eq + Hash>: EntityAndGlobalEntityConverter<E> {
    fn component_kinds(&self, entity: &E) -> Option<Vec<ComponentKind>>;
    fn to_global_entity_converter(&self) -> &dyn EntityAndGlobalEntityConverter<E>;
    fn new_mut_channel(&self, diff_mask_length: u8) -> Arc<RwLock<dyn MutChannelType>>;
    fn diff_handler(&self) -> Arc<RwLock<GlobalDiffHandler<E>>>;
    fn despawn(&mut self, entity: &E);
}

pub trait EntityAndGlobalEntityConverter<E: Copy + Eq + Hash> {
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<E, EntityDoesNotExistError>;
    fn entity_to_global_entity(&self, entity: &E) -> Result<GlobalEntity, EntityDoesNotExistError>;
}

pub trait LocalEntityAndGlobalEntityConverter {
    fn global_entity_to_local_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<LocalEntity, EntityDoesNotExistError>;
    fn local_entity_to_global_entity(
        &self,
        local_entity: &LocalEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError>;
}

pub trait LocalEntityConverter<E: Copy + Eq + Hash> {
    fn entity_to_local_entity(&self, entity: &E) -> Result<LocalEntity, EntityDoesNotExistError>;
    fn local_entity_to_entity(
        &self,
        local_entity: &LocalEntity,
    ) -> Result<E, EntityDoesNotExistError>;
}

pub struct FakeEntityConverter;

impl LocalEntityAndGlobalEntityConverter for FakeEntityConverter {
    fn global_entity_to_local_entity(
        &self,
        _: &GlobalEntity,
    ) -> Result<LocalEntity, EntityDoesNotExistError> {
        Ok(LocalEntity::Host(0))
    }

    fn local_entity_to_global_entity(
        &self,
        _: &LocalEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        Ok(GlobalEntity::from_u64(0))
    }
}

pub struct EntityConverter<'a, 'b, E: Eq + Copy + Hash> {
    global_entity_converter: &'a dyn EntityAndGlobalEntityConverter<E>,
    local_entity_converter: &'b dyn LocalEntityConverter<E>,
}

impl<'a, 'b, E: Eq + Copy + Hash> EntityConverter<'a, 'b, E> {
    pub fn new(
        global_entity_converter: &'a dyn EntityAndGlobalEntityConverter<E>,
        local_entity_converter: &'b dyn LocalEntityConverter<E>,
    ) -> Self {
        Self {
            global_entity_converter,
            local_entity_converter,
        }
    }
}

impl<'a, 'b, E: Copy + Eq + Hash> LocalEntityAndGlobalEntityConverter
    for EntityConverter<'a, 'b, E>
{
    fn global_entity_to_local_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<LocalEntity, EntityDoesNotExistError> {
        if let Ok(entity) = self
            .global_entity_converter
            .global_entity_to_entity(global_entity)
        {
            return self.local_entity_converter.entity_to_local_entity(&entity);
        }
        return Err(EntityDoesNotExistError);
    }

    fn local_entity_to_global_entity(
        &self,
        local_entity: &LocalEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        if let Ok(entity) = self
            .local_entity_converter
            .local_entity_to_entity(local_entity)
        {
            return self
                .global_entity_converter
                .entity_to_global_entity(&entity);
        }
        return Err(EntityDoesNotExistError);
    }
}

// Probably only should be used for writing messages
pub struct EntityConverterMut<'a, 'b, E: Eq + Copy + Hash> {
    global_entity_converter: &'a dyn EntityAndGlobalEntityConverter<E>,
    local_world_manager: &'b mut LocalWorldManager<E>,
}

impl<'a, 'b, E: Eq + Copy + Hash> EntityConverterMut<'a, 'b, E> {
    pub fn new(
        global_entity_converter: &'a dyn EntityAndGlobalEntityConverter<E>,
        local_world_manager: &'b mut LocalWorldManager<E>,
    ) -> Self {
        Self {
            global_entity_converter,
            local_world_manager,
        }
    }
}

pub trait LocalEntityAndGlobalEntityConverterMut {
    fn global_entity_to_local_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<LocalEntity, EntityDoesNotExistError>;
    fn local_entity_to_global_entity(
        &self,
        local_entity: &LocalEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError>;
}

impl<'a, 'b, E: Copy + Eq + Hash> LocalEntityAndGlobalEntityConverterMut
    for EntityConverterMut<'a, 'b, E>
{
    fn global_entity_to_local_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<LocalEntity, EntityDoesNotExistError> {
        if let Ok(entity) = self
            .global_entity_converter
            .global_entity_to_entity(global_entity)
        {
            return self.local_world_manager.entity_to_local_entity(&entity);
        }
        return Err(EntityDoesNotExistError);
    }

    fn local_entity_to_global_entity(
        &self,
        local_entity: &LocalEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        if let Ok(entity) = self
            .local_world_manager
            .local_entity_to_entity(local_entity)
        {
            return self
                .global_entity_converter
                .entity_to_global_entity(&entity);
        }
        return Err(EntityDoesNotExistError);
    }
}
