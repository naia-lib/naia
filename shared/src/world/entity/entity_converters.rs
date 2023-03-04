use std::hash::Hash;

use crate::{
    bigmap::BigMapKey,
    world::entity::{
        entity_handle::EntityHandle, error::EntityDoesNotExistError, net_entity::NetEntity,
    },
};

pub trait EntityHandleConverter<E: Copy + Eq + Hash> {
    fn handle_to_entity(&self, entity_handle: &EntityHandle) -> E;
    fn entity_to_handle(&self, entity: &E) -> Result<EntityHandle, EntityDoesNotExistError>;
}

pub trait NetEntityHandleConverter {
    fn handle_to_net_entity(&self, entity_handle: &EntityHandle) -> NetEntity;
    fn net_entity_to_handle(
        &self,
        net_entity: &NetEntity,
    ) -> Result<EntityHandle, EntityDoesNotExistError>;
}

pub trait NetEntityConverter<E: Copy + Eq + Hash> {
    fn entity_to_net_entity(&self, entity: &E) -> NetEntity;
    fn net_entity_to_entity(&self, net_entity: &NetEntity) -> E;
}

pub struct FakeEntityConverter;

impl NetEntityHandleConverter for FakeEntityConverter {
    fn handle_to_net_entity(&self, _: &EntityHandle) -> NetEntity {
        NetEntity::from(0)
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
    fn handle_to_net_entity(&self, entity_handle: &EntityHandle) -> NetEntity {
        let entity = self.handle_converter.handle_to_entity(entity_handle);
        self.net_entity_converter.entity_to_net_entity(&entity)
    }

    fn net_entity_to_handle(
        &self,
        net_entity: &NetEntity,
    ) -> Result<EntityHandle, EntityDoesNotExistError> {
        let entity = self.net_entity_converter.net_entity_to_entity(net_entity);
        self.handle_converter.entity_to_handle(&entity)
    }
}