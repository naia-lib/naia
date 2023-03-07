use std::{
    hash::Hash,
    sync::{Arc, RwLock},
};

use crate::{
    bigmap::BigMapKey,
    world::{
        entity::{
            entity_handle::EntityHandle, error::EntityDoesNotExistError, net_entity::NetEntity,
        },
        host::mut_channel::MutChannelType,
    },
    ComponentKind,
};

pub trait GlobalWorldManagerType<E: Copy + Eq + Hash>: EntityHandleConverter<E> {
    fn component_kinds(&self, entity: &E) -> Option<Vec<ComponentKind>>;
    fn to_handle_converter(&self) -> &dyn EntityHandleConverter<E>;
    fn new_mut_channel(&self, diff_mask_length: u8) -> Arc<RwLock<dyn MutChannelType>>;
}

pub trait EntityHandleConverter<E: Copy + Eq + Hash> {
    fn handle_to_entity(&self, entity_handle: &EntityHandle) -> Result<E, EntityDoesNotExistError>;
    fn entity_to_handle(&self, entity: &E) -> Result<EntityHandle, EntityDoesNotExistError>;
}

pub trait NetEntityHandleConverter {
    fn handle_to_net_entity(
        &self,
        entity_handle: &EntityHandle,
    ) -> Result<NetEntity, EntityDoesNotExistError>;
    fn net_entity_to_handle(
        &self,
        net_entity: &NetEntity,
    ) -> Result<EntityHandle, EntityDoesNotExistError>;
}

pub trait NetEntityConverter<E: Copy + Eq + Hash> {
    fn entity_to_net_entity(&self, entity: &E) -> Result<NetEntity, EntityDoesNotExistError>;
    fn net_entity_to_entity(&self, net_entity: &NetEntity) -> Result<E, EntityDoesNotExistError>;
}

pub struct FakeEntityConverter;

impl NetEntityHandleConverter for FakeEntityConverter {
    fn handle_to_net_entity(&self, _: &EntityHandle) -> Result<NetEntity, EntityDoesNotExistError> {
        Ok(NetEntity::from(0))
    }

    fn net_entity_to_handle(&self, _: &NetEntity) -> Result<EntityHandle, EntityDoesNotExistError> {
        Ok(EntityHandle::from_u64(0))
    }
}

pub struct EntityConverter<'a, 'b, E: Eq + Copy + Hash> {
    handle_converter: &'a dyn EntityHandleConverter<E>,
    net_entity_converter: &'b dyn NetEntityConverter<E>,
}

impl<'a, 'b, E: Eq + Copy + Hash> EntityConverter<'a, 'b, E> {
    pub fn new(
        handle_converter: &'a dyn EntityHandleConverter<E>,
        net_entity_converter: &'b dyn NetEntityConverter<E>,
    ) -> Self {
        Self {
            handle_converter,
            net_entity_converter,
        }
    }
}

impl<'a, 'b, E: Copy + Eq + Hash> NetEntityHandleConverter for EntityConverter<'a, 'b, E> {
    fn handle_to_net_entity(
        &self,
        entity_handle: &EntityHandle,
    ) -> Result<NetEntity, EntityDoesNotExistError> {
        if let Ok(entity) = self.handle_converter.handle_to_entity(entity_handle) {
            return self.net_entity_converter.entity_to_net_entity(&entity);
        }
        return Err(EntityDoesNotExistError);
    }

    fn net_entity_to_handle(
        &self,
        net_entity: &NetEntity,
    ) -> Result<EntityHandle, EntityDoesNotExistError> {
        if let Ok(entity) = self.net_entity_converter.net_entity_to_entity(net_entity) {
            return self.handle_converter.entity_to_handle(&entity);
        }
        return Err(EntityDoesNotExistError);
    }
}
