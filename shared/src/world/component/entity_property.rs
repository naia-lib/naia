use std::hash::Hash;

use naia_serde::{BitReader, BitWrite, BitWriter, ConstBitLength, Serde, SerdeErr};

use crate::world::{
    component::{property::Property, property_mutate::PropertyMutator},
    entity::{
        entity_converters::{EntityHandleConverter, NetEntityHandleConverter},
        entity_handle::EntityHandle,
        net_entity::NetEntity,
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

    pub fn new_empty() -> Self {
        Self {
            handle_prop: Property::<Option<EntityHandle>>::new(None, 0),
        }
    }

    pub fn mirror(&mut self, other: &EntityProperty) {
        *self.handle_prop = other.handle();
    }

    pub fn handle(&self) -> Option<EntityHandle> {
        *self.handle_prop
    }

    // Serialization / deserialization

    pub fn bit_length(&self) -> u32 {
        return <Option<NetEntity> as ConstBitLength>::const_bit_length();
    }

    pub fn write(&self, writer: &mut dyn BitWrite, converter: &dyn NetEntityHandleConverter) {
        (*self.handle_prop)
            .map(|handle| converter.handle_to_net_entity(&handle))
            .ser(writer);
    }

    pub fn new_read(
        reader: &mut BitReader,
        mutator_index: u8,
        converter: &dyn NetEntityHandleConverter,
    ) -> Result<Self, SerdeErr> {
        if let Some(net_entity) = Option::<NetEntity>::de(reader)? {
            if let Ok(handle) = converter.net_entity_to_handle(&net_entity) {
                let mut new_prop = Self::new(mutator_index);
                *new_prop.handle_prop = Some(handle);
                Ok(new_prop)
            } else {
                panic!("Could not find Entity to associate with incoming EntityProperty value!");
            }
        } else {
            let mut new_prop = Self::new(mutator_index);
            *new_prop.handle_prop = None;
            Ok(new_prop)
        }
    }

    pub fn read_write(reader: &mut BitReader, writer: &mut BitWriter) -> Result<(), SerdeErr> {
        Option::<NetEntity>::de(reader)?.ser(writer);
        Ok(())
    }

    pub fn read(
        &mut self,
        reader: &mut BitReader,
        converter: &dyn NetEntityHandleConverter,
    ) -> Result<(), SerdeErr> {
        if let Some(net_entity) = Option::<NetEntity>::de(reader)? {
            if let Ok(handle) = converter.net_entity_to_handle(&net_entity) {
                *self.handle_prop = Some(handle);
            } else {
                panic!("Could not find Entity to associate with incoming EntityProperty value!");
            }
        } else {
            *self.handle_prop = None;
        }
        Ok(())
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
        if let Ok(new_handle) = handler.entity_to_handle(entity) {
            *self.handle_prop = Some(new_handle);
        } else {
            panic!("Could not find Entity, in order to set the EntityProperty value!")
        }
    }
}
