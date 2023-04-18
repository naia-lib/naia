use std::hash::Hash;

use naia_serde::{BitCounter, BitReader, BitWrite, BitWriter, Serde, SerdeErr};

use crate::world::{
    component::{property::Property, property_mutate::PropertyMutator},
    entity::{
        entity_converters::{EntityAndGlobalEntityConverter, LocalEntityAndGlobalEntityConverter},
        global_entity::GlobalEntity,
        local_entity::LocalEntity,
    },
};

#[derive(Clone)]
pub struct EntityRelation {
    global_entity: Option<GlobalEntity>,
}

impl EntityRelation {

    pub fn new() -> Self {
        Self {
            global_entity: None,
        }
    }

    pub fn mirror(&mut self, other: &EntityRelation) {
        self.global_entity = other.global_entity();
    }

    pub fn global_entity(&self) -> Option<GlobalEntity> {
        self.global_entity
    }

    // Serialization / deserialization

    pub fn write(
        &self,
        writer: &mut dyn BitWrite,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) {
        if let Some(global_entity) = &self.global_entity {
            if let Ok(local_entity) = converter.global_entity_to_local_entity(global_entity) {
                // Must reverse the LocalEntity because the Host<->Remote
                // relationship inverts after this data goes over the wire
                let reversed_local_entity = local_entity.to_reversed();

                true.ser(writer);
                reversed_local_entity.owned_ser(writer);
                return;
            }
        }
        false.ser(writer);
    }

    pub fn bit_length(&self, converter: &dyn LocalEntityAndGlobalEntityConverter) -> u32 {
        let mut bit_counter = BitCounter::new(0, 0, u32::MAX);
        self.write(&mut bit_counter, converter);
        return bit_counter.bits_needed();
    }

    pub fn new_read(
        reader: &mut BitReader,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) -> Result<Self, SerdeErr> {
        let exists = bool::de(reader)?;
        if exists {
            let local_entity = LocalEntity::owned_de(reader)?;
            if let Ok(global_entity) = converter.local_entity_to_global_entity(&local_entity) {
                let mut new_prop = Self::new();
                new_prop.global_entity = Some(global_entity);
                Ok(new_prop)
            } else {
                panic!("Could not find GlobalEntity to associate with incoming EntityProperty value!");
            }
        } else {
            let mut new_prop = Self::new();
            new_prop.global_entity = None;
            Ok(new_prop)
        }
    }

    pub fn read_write(reader: &mut BitReader, writer: &mut BitWriter) -> Result<(), SerdeErr> {
        let exists = bool::de(reader)?;
        exists.ser(writer);
        if exists {
            let local_entity = LocalEntity::owned_de(reader)?;
            local_entity.owned_ser(writer);
        }
        Ok(())
    }

    // Comparison

    pub fn equals(&self, other: &EntityRelation) -> bool {
        if let Some(global_entity) = self.global_entity {
            if let Some(other_global_entity) = other.global_entity {
                return global_entity == other_global_entity;
            }
            return false;
        }
        other.global_entity.is_none()
    }

    // Internal

    pub fn get<E: Copy + Eq + Hash>(
        &self,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
    ) -> Option<E> {
        if let Some(global_entity) = self.global_entity {
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
            self.global_entity = Some(new_global_entity);
        } else {
            panic!("Could not find Entity, in order to set the EntityProperty value!")
        }
    }
}
