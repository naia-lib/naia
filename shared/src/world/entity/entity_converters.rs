use std::{
    hash::Hash,
    sync::{Arc, RwLock},
};

use crate::{
    bigmap::BigMapKey,
    world::{
        entity::{
            error::EntityDoesNotExistError, global_entity::GlobalEntity, net_entity::OwnedNetEntity,
        },
        host::mut_channel::MutChannelType,
    },
    ComponentKind, GlobalDiffHandler,
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

pub trait NetEntityAndGlobalEntityConverter {
    fn global_entity_to_net_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<OwnedNetEntity, EntityDoesNotExistError>;
    fn net_entity_to_global_entity(
        &self,
        net_entity: &OwnedNetEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError>;
}

pub trait NetEntityConverter<E: Copy + Eq + Hash> {
    fn entity_to_net_entity(&self, entity: &E) -> Result<OwnedNetEntity, EntityDoesNotExistError>;
    fn net_entity_to_entity(
        &self,
        net_entity: &OwnedNetEntity,
    ) -> Result<E, EntityDoesNotExistError>;
}

pub struct FakeEntityConverter;

impl NetEntityAndGlobalEntityConverter for FakeEntityConverter {
    fn global_entity_to_net_entity(
        &self,
        _: &GlobalEntity,
    ) -> Result<OwnedNetEntity, EntityDoesNotExistError> {
        Ok(OwnedNetEntity::Host(0))
    }

    fn net_entity_to_global_entity(
        &self,
        _: &OwnedNetEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        Ok(GlobalEntity::from_u64(0))
    }
}

pub struct EntityConverter<'a, 'b, E: Eq + Copy + Hash> {
    global_entity_converter: &'a dyn EntityAndGlobalEntityConverter<E>,
    net_entity_converter: &'b dyn NetEntityConverter<E>,
}

impl<'a, 'b, E: Eq + Copy + Hash> EntityConverter<'a, 'b, E> {
    pub fn new(
        global_entity_converter: &'a dyn EntityAndGlobalEntityConverter<E>,
        net_entity_converter: &'b dyn NetEntityConverter<E>,
    ) -> Self {
        Self {
            global_entity_converter,
            net_entity_converter,
        }
    }
}

impl<'a, 'b, E: Copy + Eq + Hash> NetEntityAndGlobalEntityConverter for EntityConverter<'a, 'b, E> {
    fn global_entity_to_net_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<OwnedNetEntity, EntityDoesNotExistError> {
        if let Ok(entity) = self
            .global_entity_converter
            .global_entity_to_entity(global_entity)
        {
            return self.net_entity_converter.entity_to_net_entity(&entity);
        }
        return Err(EntityDoesNotExistError);
    }

    fn net_entity_to_global_entity(
        &self,
        net_entity: &OwnedNetEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        if let Ok(entity) = self.net_entity_converter.net_entity_to_entity(net_entity) {
            return self
                .global_entity_converter
                .entity_to_global_entity(&entity);
        }
        return Err(EntityDoesNotExistError);
    }
}
