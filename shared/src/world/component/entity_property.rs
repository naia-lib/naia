use std::hash::Hash;

use naia_serde::{BitCounter, BitReader, BitWrite, BitWriter, Serde, SerdeErr};

use crate::world::{
    component::{property::Property, property_mutate::PropertyMutator},
    entity::{
        entity_converters::{EntityAndGlobalEntityConverter, NetEntityAndGlobalEntityConverter},
        global_entity::GlobalEntity,
        owned_net_entity::OwnedNetEntity,
    },
};

#[derive(Clone)]
pub struct EntityProperty {
    global_entity_prop: Property<Option<GlobalEntity>>,
}

impl EntityProperty {
    pub fn new(mutator_index: u8) -> Self {
        Self {
            global_entity_prop: Property::<Option<GlobalEntity>>::new(None, mutator_index),
        }
    }

    pub fn new_empty() -> Self {
        Self {
            global_entity_prop: Property::<Option<GlobalEntity>>::new(None, 0),
        }
    }

    pub fn mirror(&mut self, other: &EntityProperty) {
        *self.global_entity_prop = other.global_entity();
    }

    pub fn global_entity(&self) -> Option<GlobalEntity> {
        *self.global_entity_prop
    }

    // Serialization / deserialization

    pub fn write(
        &self,
        writer: &mut dyn BitWrite,
        converter: &dyn NetEntityAndGlobalEntityConverter,
    ) {
        if let Some(global_entity) = *self.global_entity_prop {
            if let Ok(net_entity) = converter.global_entity_to_net_entity(&global_entity) {
                // Must reverse the OwnedNetEntity because the Host<->Remote
                // relationship inverts after this data goes over the wire
                let reversed_net_entity = net_entity.to_reversed();

                let opt = Some(reversed_net_entity);
                opt.ser(writer);
                return;
            }
        }
        let opt: Option<OwnedNetEntity> = None;
        opt.ser(writer);
    }

    pub fn bit_length(&self, converter: &dyn NetEntityAndGlobalEntityConverter) -> u32 {
        let mut bit_counter = BitCounter::new(0, 0, u32::MAX);
        self.write(&mut bit_counter, converter);
        return bit_counter.bits_needed();
    }

    pub fn new_read(
        reader: &mut BitReader,
        mutator_index: u8,
        converter: &dyn NetEntityAndGlobalEntityConverter,
    ) -> Result<Self, SerdeErr> {
        if let Some(net_entity) = Option::<OwnedNetEntity>::de(reader)? {
            if let Ok(global_entity) = converter.net_entity_to_global_entity(&net_entity) {
                let mut new_prop = Self::new(mutator_index);
                *new_prop.global_entity_prop = Some(global_entity);
                Ok(new_prop)
            } else {
                panic!("Could not find Entity to associate with incoming EntityProperty value!");
            }
        } else {
            let mut new_prop = Self::new(mutator_index);
            *new_prop.global_entity_prop = None;
            Ok(new_prop)
        }
    }

    pub fn read_write(reader: &mut BitReader, writer: &mut BitWriter) -> Result<(), SerdeErr> {
        Option::<OwnedNetEntity>::de(reader)?.ser(writer);
        Ok(())
    }

    pub fn read(
        &mut self,
        reader: &mut BitReader,
        converter: &dyn NetEntityAndGlobalEntityConverter,
    ) -> Result<(), SerdeErr> {
        if let Some(net_entity) = Option::<OwnedNetEntity>::de(reader)? {
            if let Ok(global_entity) = converter.net_entity_to_global_entity(&net_entity) {
                *self.global_entity_prop = Some(global_entity);
            } else {
                panic!("Could not find Entity to associate with incoming EntityProperty value!");
            }
        } else {
            *self.global_entity_prop = None;
        }
        Ok(())
    }

    // Comparison

    pub fn equals(&self, other: &EntityProperty) -> bool {
        if let Some(global_entity) = *self.global_entity_prop {
            if let Some(other_global_entity) = *other.global_entity_prop {
                return global_entity == other_global_entity;
            }
            return false;
        }
        other.global_entity_prop.is_none()
    }

    // Internal

    pub fn set_mutator(&mut self, mutator: &PropertyMutator) {
        self.global_entity_prop.set_mutator(mutator);
    }

    pub fn get<E: Copy + Eq + Hash>(
        &self,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
    ) -> Option<E> {
        if let Some(global_entity) = *self.global_entity_prop {
            if let Ok(entity) = converter.global_entity_to_entity(&global_entity) {
                return Some(entity);
            }
        }
        return None;
    }

    pub fn set<E: Copy + Eq + Hash>(
        &mut self,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        entity: &E,
    ) {
        if let Ok(new_global_entity) = converter.entity_to_global_entity(entity) {
            *self.global_entity_prop = Some(new_global_entity);
        } else {
            panic!("Could not find Entity, in order to set the EntityProperty value!")
        }
    }
}
