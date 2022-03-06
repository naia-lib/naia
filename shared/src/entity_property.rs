use std::ops::DerefMut;

use naia_serde::{BitReader, BitWrite};

use crate::{property_mutate::PropertyMutator, EntityHandle, NetEntity, Property};

#[derive(Eq, PartialEq, Clone)]
enum NeedToSet {
    NetEntity,
    Handle,
}

#[derive(Clone)]
pub struct EntityProperty {
    net_entity: Property<NetEntity>,
    handle: EntityHandle,
    need_to_set: NeedToSet,
}

impl EntityProperty {
    pub fn new(handle: EntityHandle, mutator_index: u8) -> Self {
        Self {
            net_entity: Property::<NetEntity>::new(NetEntity::default(), mutator_index),
            handle,
            need_to_set: NeedToSet::NetEntity,
        }
    }

    pub fn mirror(&mut self, other: &EntityProperty) {
        self.set_handle(other.get_handle());
    }

    // Serialization / deserialization

    pub fn write<S: BitWrite>(&self, writer: &mut S) {
        if self.need_to_set == NeedToSet::NetEntity {
            panic!("have not updated inner in time for a write!");
        }
        self.net_entity.write(writer);
    }

    pub fn new_read(reader: &mut BitReader, mutator_index: u8) -> Self {
        Self {
            net_entity: Property::<NetEntity>::new_read(reader, mutator_index),
            handle: EntityHandle::empty(),
            need_to_set: NeedToSet::Handle,
        }
    }

    pub fn read(&mut self, reader: &mut BitReader) {
        self.net_entity.read(reader);
        self.need_to_set = NeedToSet::Handle;
    }

    // Comparison

    pub fn equals(&self, other: &EntityProperty) -> bool {
        return self.net_entity.equals(&other.net_entity) && self.handle == other.handle;
    }

    // Internal

    pub fn set_mutator(&mut self, mutator: &PropertyMutator) {
        self.net_entity.set_mutator(mutator);
    }

    pub fn get_handle(&self) -> EntityHandle {
        if self.need_to_set == NeedToSet::Handle {
            panic!("have not updated outer in time for a read!");
        }

        return self.handle;
    }

    pub fn set_handle(&mut self, handle: EntityHandle) {
        self.handle = handle;

        // flag net entity for update
        self.net_entity.deref_mut();

        self.need_to_set = NeedToSet::NetEntity;
    }
}
