use std::collections::VecDeque;
use std::hash::Hash;

use naia_serde::{BitReader, BitWrite, BitWriter, Serde, SerdeErr, UnsignedVariableInteger};

use crate::{
    bigmap::BigMapKey,
    protocol::{
        entity_handle::EntityHandle, net_entity::NetEntity, property::Property,
        property_mutate::PropertyMutator,
    },
};

use crate::protocol::replicable_property::{ReplicableEntityProperty, ReplicableProperty};

#[derive(Clone)]
pub struct EntityProperty {
    // note: the Option<> is there just because we initialize the value as None.
    // I think it would be easier to never use 0 as entityHandle, and just use the default-value as
    // uninitialized
    // We should also update the new_complete function to directly support entity initialization as well
    // We don't really allow partial replicate messages anyway!
    handle_prop: Property<Option<EntityHandle>>,
}

impl EntityProperty {
    pub fn handle(&self) -> Option<EntityHandle> {
        *self.handle_prop
    }

    pub fn get<E: Copy + Eq + Hash>(&self, handler: &dyn EntityHandleConverter<E>) -> Option<E> {
        (*self.handle_prop).map(|handle| handler.handle_to_entity(&handle))
    }

    pub fn set<E: Copy + Eq + Hash>(&mut self, handler: &dyn EntityHandleConverter<E>, entity: &E) {
        let new_handle = handler.entity_to_handle(entity);
        *self.handle_prop = Some(new_handle);
    }
}


impl ReplicableEntityProperty for EntityProperty {
    fn new(mutator_index: u8) -> Self {
        Self {
            handle_prop: Property::<Option<EntityHandle>>::new(None, mutator_index),
        }
    }

    fn mirror(&mut self, other: &Self) {
        *self.handle_prop = other.handle();
    }

    // Serialization / deserialization

    fn write(&self, writer: &mut dyn BitWrite, converter: &dyn NetEntityHandleConverter) {
        (*self.handle_prop)
            .map(|handle| converter.handle_to_net_entity(&handle))
            .ser(writer);
    }

    fn new_read(
        reader: &mut BitReader,
        mutator_index: u8,
        converter: &dyn NetEntityHandleConverter,
    ) -> Result<Self, SerdeErr> {
        if let Some(net_entity) = Option::<NetEntity>::de(reader)? {
            let handle = converter.net_entity_to_handle(&net_entity);
            let mut new_prop = Self::new(mutator_index);
            *new_prop.handle_prop = Some(handle);
            Ok(new_prop)
        } else {
            let mut new_prop = Self::new(mutator_index);
            *new_prop.handle_prop = None;
            Ok(new_prop)
        }
    }

    fn read_write(reader: &mut BitReader, writer: &mut BitWriter) -> Result<(), SerdeErr> {
        Option::<NetEntity>::de(reader)?.ser(writer);
        Ok(())
    }

    fn read(
        &mut self,
        reader: &mut BitReader,
        converter: &dyn NetEntityHandleConverter,
    ) -> Result<(), SerdeErr> {
        if let Some(net_entity) = Option::<NetEntity>::de(reader)? {
            let handle = converter.net_entity_to_handle(&net_entity);
            *self.handle_prop = Some(handle);
        } else {
            *self.handle_prop = None;
        }
        Ok(())
    }

    // Comparison

    fn equals(&self, other: &EntityProperty) -> bool {
        if let Some(handle) = *self.handle_prop {
            if let Some(other_handle) = *other.handle_prop {
                return handle == other_handle;
            }
            return false;
        }
        other.handle_prop.is_none()
    }

    // Entities

    fn entities(&self) -> Vec<EntityHandle> {
        let mut output = Vec::new();
        if let Some(handle) = self.handle() {
            output.push(handle);
        }
        output
    }

    // Internal

    fn set_mutator(&mut self, mutator: &PropertyMutator) {
        self.handle_prop.set_mutator(mutator);
    }
}


// NOTE: I would have liked to implement this as
// VecDeque<EntityProperty> so that I could simply re-use all the EntityProperty functions
// Figure out a better way to re-use code

// However we need Property<VecDeque<EntityHandle>> because we need the mutator (via DerefMut)
// to be alerted whenever anything changes in the VecDeque

#[derive(Clone, Debug, Default)]
// Reflect not supported for VecDeque
// #[cfg_attr(feature = "bevy_support", derive(Reflect))]
pub struct VecDequeEntityProperty(Property<VecDeque<EntityHandle>>);

impl VecDequeEntityProperty {
    pub fn inner(&self) -> VecDeque<EntityHandle> {
        // TODO: should we get rid of this clone?
        (*self.0).clone()
    }

    pub fn get<E: Copy + Eq + Hash>(&self, handler: &dyn EntityHandleConverter<E>) -> VecDeque<E> {
        self.0.iter().map(|handle| handler.handle_to_entity(handle)).collect()
    }

    pub fn set<E: Copy + Eq + Hash>(&mut self, handler: &dyn EntityHandleConverter<E>, entities: &VecDeque<E>) {
        let mut queue = VecDeque::<EntityHandle>::new();
        entities.iter().for_each(|e| {
            let new_handle = handler.entity_to_handle(e);
            queue.push_back(new_handle);
        });
        *self.0 = queue;
    }

    pub fn push_front<E: Copy + Eq + Hash>(&mut self, handler: &dyn EntityHandleConverter<E>, entity: &E) {
        let new_handle = handler.entity_to_handle(entity);
        self.0.push_front(new_handle);
    }

    pub fn push_back<E: Copy + Eq + Hash>(&mut self, handler: &dyn EntityHandleConverter<E>, entity: &E) {
        let new_handle = handler.entity_to_handle(entity);
        self.0.push_back(new_handle);
    }
}


// TODO: maybe use a wrapper instead of directly using deque?
//  because we cannot shadow some functions like 'new', and because Self has to be Sized
impl ReplicableEntityProperty for VecDequeEntityProperty {
    fn new(mutator_index: u8) -> Self {
        Self(Property::<VecDeque<EntityHandle>>::new(VecDeque::new(), mutator_index))
    }

    fn mirror(&mut self, other: &Self) {
        *self.0 = other.inner();
    }

    fn write(&self, writer: &mut dyn BitWrite, converter: &dyn NetEntityHandleConverter) {
        let length = UnsignedVariableInteger::<5>::new(self.0.len() as u64);
        length.ser(writer);
        self.0.iter().for_each(|e| e.write(writer, converter));
    }

    fn new_read(reader: &mut BitReader, mutator_index: u8, converter: &dyn NetEntityHandleConverter) -> Result<Self, SerdeErr> {
        let length_int = UnsignedVariableInteger::<5>::de(reader)?;
        let length_usize = length_int.get() as usize;
        let mut output = VecDeque::with_capacity(length_usize);
        for _ in 0..length_usize {
            let entity_handle = EntityHandle::read(reader, converter)?;
            output.push_back(entity_handle);
        }
        let mut res = Self::new(mutator_index);
        *res.0 = output;
        Ok(res)
    }

    fn read_write(reader: &mut BitReader, writer: &mut BitWriter) -> Result<(), SerdeErr> {
        let length_int = UnsignedVariableInteger::<5>::de(reader)?;
        length_int.ser(writer);
        let length_usize = length_int.get() as usize;
        for _ in 0..length_usize {
            EntityHandle::read_write(reader, writer)?;
        }
        Ok(())
    }

    fn read(&mut self, reader: &mut BitReader, converter: &dyn NetEntityHandleConverter) -> Result<(), SerdeErr> {
        let length_int = UnsignedVariableInteger::<5>::de(reader)?;
        let length_usize = length_int.get() as usize;
        let mut output = VecDeque::with_capacity(length_usize);
        for _ in 0..length_usize {
            let entity_handle = EntityHandle::read(reader, converter)?;
            output.push_back(entity_handle);
        }
        *self.0 = output;
        Ok(())
    }

    fn equals(&self, other: &Self) -> bool {
        *self.0 == *other.0
    }

    fn entities(&self) -> Vec<EntityHandle> {
        self.inner().into()
    }

    fn set_mutator(&mut self, mutator: &PropertyMutator) {
        self.0.set_mutator(mutator)
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
