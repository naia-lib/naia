use std::hash::Hash;
use std::marker::PhantomData;

use naia_serde::{BitReader, BitWrite, Serde};

use crate::{property_mutate::PropertyMutator, EntityHandle, Property, NetEntity};
use crate::entity_property::NeedToSet::{HandleProperty, Uninit};

#[derive(Eq, PartialEq, Clone, Copy)]
enum NeedToSet {
    Uninit,
    HandleProperty,
    NetEntity,
    None,
}

#[derive(Clone)]
pub struct EntityProperty {
    handle_prop: Property<EntityHandle>,
    net_entity: Option<NetEntity>,
    need_to_set: NeedToSet,
}

impl EntityProperty {
    pub fn new(mutator_index: u8) -> Self {
        Self {
            handle_prop: Property::<EntityHandle>::new(EntityHandle::empty(), mutator_index),
            net_entity: None,
            need_to_set: Uninit,
        }
    }

    pub fn mirror(&mut self, other: &EntityProperty) {
        *self.handle_prop = other.handle();
        self.net_entity = other.net_entity;
        self.need_to_set = other.need_to_set;
    }

    pub(crate) fn handle(&self) -> EntityHandle {
        *self.handle_prop
    }

    pub fn prewrite(&mut self, entity_converter: &dyn NetEntityHandleConverter) {
        if self.need_to_set == NeedToSet::NetEntity {

            if let Some(net_entity) = entity_converter.handle_to_net_entity(&self.handle_prop) {
                self.net_entity = Some(net_entity);
            } else {
                self.net_entity = None;
            }

            self.need_to_set = NeedToSet::None;
        }
    }

    pub fn postread(&mut self, entity_converter: &mut dyn NetEntityHandleConverter) {
        if self.need_to_set == HandleProperty {

            if let Some(net_entity) = &self.net_entity {
                *self.handle_prop = entity_converter.net_entity_to_handle(net_entity);
            } else {
                *self.handle_prop = EntityHandle::empty();
            }

            self.need_to_set = NeedToSet::None;
        }
    }

    // Serialization / deserialization

    pub fn write<S: BitWrite>(&self, writer: &mut S) {
        if self.need_to_set == NeedToSet::NetEntity {
            panic!("Still need to sync with World!");
        }
        self.net_entity.ser(writer);
    }

    pub fn new_read(reader: &mut BitReader, mutator_index: u8) -> Self {
        let mut new_prop = Self::new(mutator_index);
        new_prop.read(reader);
        return new_prop;
    }

    pub fn read(&mut self, reader: &mut BitReader) {
        self.net_entity = Some(NetEntity::de(reader).unwrap());
        self.need_to_set = NeedToSet::HandleProperty;
    }

    // Comparison

    pub fn equals(&self, other: &EntityProperty) -> bool {
        return self.handle_prop.equals(&other.handle_prop)
            && self.net_entity == other.net_entity
            && self.need_to_set == other.need_to_set;
    }

    // Internal

    pub fn set_mutator(&mut self, mutator: &PropertyMutator) {
        self.handle_prop.set_mutator(mutator);
    }

    pub fn get<'h, E: Copy + Eq + Hash>(
        &self,
        handler: &'h dyn EntityHandleConverter<E>,
    ) -> Option<&'h E> {
        if self.need_to_set == NeedToSet::HandleProperty {
            panic!("EntityProperty still needs to be synced with World!");
        }
        let handle = *self.handle_prop;
        return handler.handle_to_entity(&handle);
    }

    pub fn set<E: Copy + Eq + Hash>(
        &mut self,
        handler: &mut dyn EntityHandleConverter<E>,
        entity: &E,
    ) {
        let new_handle = handler.entity_to_handle(entity);
        *self.handle_prop = new_handle;
        self.need_to_set = NeedToSet::NetEntity;
    }
}

pub trait EntityHandleConverter<E: Copy + Eq + Hash> {
    fn handle_to_entity(&self, entity_handle: &EntityHandle) -> Option<&E>;
    fn entity_to_handle(&mut self, entity: &E) -> EntityHandle;
}

pub trait NetEntityHandleConverter {
    fn handle_to_net_entity(&self, entity_handle: &EntityHandle) -> Option<NetEntity>;
    fn net_entity_to_handle(&mut self, net_entity: &NetEntity) -> EntityHandle;
}

pub trait NetEntityConverter<E: Copy + Eq + Hash> {
    fn entity_to_net_entity(&self, entity: &E) -> NetEntity;
    fn net_entity_to_entity(&self, net_entity: &NetEntity) -> E;
}

impl<E: Copy + Eq + Hash, A: EntityHandleConverter<E>, B: NetEntityConverter<E>> NetEntityHandleConverter for (&mut A, &mut B, PhantomData<E>)
{
    fn handle_to_net_entity(&self, entity_handle: &EntityHandle) -> Option<NetEntity> {
        let entity_handle_converter = &self.0;
        let net_entity_converter = &self.1;

        if let Some(entity) = entity_handle_converter.handle_to_entity(entity_handle) {
            return Some(net_entity_converter.entity_to_net_entity(entity));
        }
        return None;
    }

    fn net_entity_to_handle(&mut self, net_entity: &NetEntity) -> EntityHandle {
        let entity_handle_converter = &mut self.0;
        let net_entity_converter = &self.1;

        let entity = net_entity_converter.net_entity_to_entity(net_entity);
        return entity_handle_converter.entity_to_handle(&entity);
    }
}