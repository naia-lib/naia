use std::hash::Hash;

use naia_serde::{BitReader, BitWrite, BitWriter, Serde};

use crate::{
    bigmap::BigMapKey,
    protocol::{
        entity_handle::EntityHandle, net_entity::NetEntity, property::Property,
        property_mutate::PropertyMutator,
    },
};

#[derive(Clone)]
pub struct EntityProperty {
    handle_prop: Property<Option<EntityHandle>>,
}

impl EntityProperty {
    pub fn new(mutator_index: u8) -> Self {
        Self {
            handle_prop: Property::<Option<EntityHandle>>::new(None, mutator_index),
        }
    }

    pub fn mirror(&mut self, other: &EntityProperty) {
        *self.handle_prop = other.handle();
    }

    pub fn handle(&self) -> Option<EntityHandle> {
        *self.handle_prop
    }

    // Serialization / deserialization

    pub fn write(&self, writer: &mut dyn BitWrite, converter: &dyn NetEntityHandleConverter) {
        (*self.handle_prop)
            .map(|handle| converter.handle_to_net_entity(&handle))
            .ser(writer);
    }

    pub fn new_read(
        reader: &mut BitReader,
        mutator_index: u8,
        converter: &dyn NetEntityHandleConverter,
    ) -> Self {
        if let Some(net_entity) = Option::<NetEntity>::de(reader).unwrap() {
            let handle = converter.net_entity_to_handle(&net_entity);
            let mut new_prop = Self::new(mutator_index);
            *new_prop.handle_prop = Some(handle);
            new_prop
        } else {
            let mut new_prop = Self::new(mutator_index);
            *new_prop.handle_prop = None;
            new_prop
        }
    }

    pub fn read_write(bit_reader: &mut BitReader, bit_writer: &mut BitWriter) {
        Option::<NetEntity>::de(bit_reader).unwrap().ser(bit_writer);
    }

    pub fn read(&mut self, reader: &mut BitReader, converter: &dyn NetEntityHandleConverter) {
        if let Some(net_entity) = Option::<NetEntity>::de(reader).unwrap() {
            let handle = converter.net_entity_to_handle(&net_entity);
            *self.handle_prop = Some(handle);
        } else {
            *self.handle_prop = None;
        }
    }

    // Comparison

    pub fn equals(&self, other: &EntityProperty) -> bool {
        if let Some(handle) = *self.handle_prop {
            if let Some(other_handle) = *other.handle_prop {
                return handle == other_handle;
            }
            return false;
        }
        other.handle_prop.is_none()
    }

    // Internal

    pub fn set_mutator(&mut self, mutator: &PropertyMutator) {
        self.handle_prop.set_mutator(mutator);
    }

    pub fn get<E: Copy + Eq + Hash>(&self, handler: &dyn EntityHandleConverter<E>) -> Option<E> {
        (*self.handle_prop).map(|handle| handler.handle_to_entity(&handle))
    }

    pub fn set<E: Copy + Eq + Hash>(&mut self, handler: &dyn EntityHandleConverter<E>, entity: &E) {
        let new_handle = handler.entity_to_handle(entity);
        *self.handle_prop = Some(new_handle);
    }
}

pub trait EntityHandleConverter<E: Copy + Eq + Hash> {
    fn handle_to_entity(&self, entity_handle: &EntityHandle) -> E;
    fn entity_to_handle(&self, entity: &E) -> EntityHandle;
}

pub trait NetEntityHandleConverter {
    fn handle_to_net_entity(&self, entity_handle: &EntityHandle) -> NetEntity;
    fn net_entity_to_handle(&self, net_entity: &NetEntity) -> EntityHandle;
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

    fn net_entity_to_handle(&self, _: &NetEntity) -> EntityHandle {
        EntityHandle::from_u64(0)
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

    fn net_entity_to_handle(&self, net_entity: &NetEntity) -> EntityHandle {
        let entity = self.net_entity_converter.net_entity_to_entity(net_entity);
        self.handle_converter.entity_to_handle(&entity)
    }
}
