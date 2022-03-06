use std::hash::Hash;

use naia_serde::{BitReader, BitWrite};

use crate::{property_mutate::PropertyMutator, EntityHandle, Property};

#[derive(Clone)]
pub struct EntityProperty {
    inner: Property<EntityHandle>,
}

impl EntityProperty {
    pub fn new(mutator_index: u8) -> Self {
        Self {
            inner: Property::<EntityHandle>::new(EntityHandle::empty(), mutator_index),
        }
    }

    pub fn mirror(&mut self, other: &EntityProperty) {
        *self.inner = other.get_inner();
    }

    pub(crate) fn get_inner(&self) -> EntityHandle {
        *self.inner
    }

    // Serialization / deserialization

    pub fn write<S: BitWrite>(&self, writer: &mut S) {
        self.inner.write(writer);
    }

    pub fn new_read(reader: &mut BitReader, mutator_index: u8) -> Self {
        Self {
            inner: Property::<EntityHandle>::new_read(reader, mutator_index),
        }
    }

    pub fn read(&mut self, reader: &mut BitReader) {
        self.inner.read(reader);
    }

    // Comparison

    pub fn equals(&self, other: &EntityProperty) -> bool {
        return self.inner.equals(&other.inner);
    }

    // Internal

    pub fn set_mutator(&mut self, mutator: &PropertyMutator) {
        self.inner.set_mutator(mutator);
    }

    pub fn get<'h, E: Copy + Eq + Hash>(
        &self,
        handler: &'h dyn EntityHandleConverter<E>,
    ) -> Option<&'h E> {
        let handle = *self.inner;
        return handler.handle_to_entity(&handle);
    }

    pub fn set<E: Copy + Eq + Hash>(
        &mut self,
        handler: &mut dyn EntityHandleConverter<E>,
        entity: &E,
    ) {
        let new_handle = handler.entity_to_handle(entity);
        *self.inner = new_handle;
    }
}

// EntityPropertyHandler
pub trait EntityHandleConverter<E: Copy + Eq + Hash> {
    fn handle_to_entity(&self, entity_handle: &EntityHandle) -> Option<&E>;
    fn entity_to_handle(&mut self, entity: &E) -> EntityHandle;
}
