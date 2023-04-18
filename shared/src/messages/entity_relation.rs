use log::warn;
use std::hash::Hash;

use naia_serde::{BitCounter, BitReader, BitWrite, Serde, SerdeErr};

use crate::world::entity::{
    entity_converters::{EntityAndGlobalEntityConverter, LocalEntityAndGlobalEntityConverter},
    global_entity::GlobalEntity,
    local_entity::LocalEntity,
};

#[derive(Clone)]
enum RelationImpl {
    HostOwned(HostOwnedRelation),
    RemoteOwned(RemoteOwnedRelation),
    RemoteWaiting(RemoteWaitingRelation),
}

#[derive(Clone)]
pub struct EntityRelation {
    inner: RelationImpl,
}

impl EntityRelation {
    pub fn new() -> Self {
        Self {
            inner: RelationImpl::HostOwned(HostOwnedRelation::new()),
        }
    }

    // Serialization / deserialization

    pub fn write(
        &self,
        writer: &mut dyn BitWrite,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) {
        match &self.inner {
            RelationImpl::HostOwned(inner) => {
                inner.write(writer, converter);
            }
            RelationImpl::RemoteOwned(_) | RelationImpl::RemoteWaiting(_) => {
                panic!("Remote Relations should never be written.");
            }
        }
    }

    pub fn bit_length(&self, converter: &dyn LocalEntityAndGlobalEntityConverter) -> u32 {
        match &self.inner {
            RelationImpl::HostOwned(inner) => inner.bit_length(converter),
            RelationImpl::RemoteOwned(_) | RelationImpl::RemoteWaiting(_) => {
                panic!(
                    "Remote Relations should never be written, so no need for their bit length."
                );
            }
        }
    }

    pub fn read(
        reader: &mut BitReader,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) -> Result<Self, SerdeErr> {
        let exists = bool::de(reader)?;
        if exists {
            let local_entity = LocalEntity::owned_de(reader)?;
            if let Ok(global_entity) = converter.local_entity_to_global_entity(&local_entity) {
                let mut new_impl = RemoteOwnedRelation::new();
                new_impl.global_entity = Some(global_entity);

                let new_self = Self {
                    inner: RelationImpl::RemoteOwned(new_impl),
                };

                Ok(new_self)
            } else {
                let new_impl = RemoteWaitingRelation::new(local_entity);

                let new_self = Self {
                    inner: RelationImpl::RemoteWaiting(new_impl),
                };

                Ok(new_self)
            }
        } else {
            let mut new_impl = RemoteOwnedRelation::new();
            new_impl.global_entity = None;

            let new_self = Self {
                inner: RelationImpl::RemoteOwned(new_impl),
            };

            Ok(new_self)
        }
    }

    // Internal

    pub fn get<E: Copy + Eq + Hash>(
        &self,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
    ) -> Option<E> {
        match &self.inner {
            RelationImpl::HostOwned(inner) => inner.get(converter),
            RelationImpl::RemoteOwned(inner) => inner.get(converter),
            RelationImpl::RemoteWaiting(_) => {
                panic!("Not ready to get RemoteWaiting Relation value!");
            }
        }
    }

    pub fn set<E: Copy + Eq + Hash>(
        &mut self,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        entity: &E,
    ) {
        match &mut self.inner {
            RelationImpl::HostOwned(inner) => {
                inner.set(converter, entity);
            }
            RelationImpl::RemoteOwned(_) | RelationImpl::RemoteWaiting(_) => {
                panic!("Remote Relations should never be set manually.");
            }
        }
    }

    // Waiting

    pub fn waiting_local_entity(&self) -> Option<LocalEntity> {
        match &self.inner {
            RelationImpl::HostOwned(_) | RelationImpl::RemoteOwned(_) => None,
            RelationImpl::RemoteWaiting(inner) => Some(inner.local_entity),
        }
    }

    pub fn waiting_complete(&mut self, converter: &dyn LocalEntityAndGlobalEntityConverter) {
        match &mut self.inner {
            RelationImpl::HostOwned(_) | RelationImpl::RemoteOwned(_) => {
                panic!("Can't complete a RemoteOwned or HostOwned Relation!");
            }
            RelationImpl::RemoteWaiting(inner) => {
                if let Ok(global_entity) =
                    converter.local_entity_to_global_entity(&inner.local_entity)
                {
                    let mut new_impl = RemoteOwnedRelation::new();
                    new_impl.global_entity = Some(global_entity);

                    self.inner = RelationImpl::RemoteOwned(new_impl);
                } else {
                    panic!("Could not find Global Entity from Local Entity! Should only call `waiting_complete` when it is known the converter will not fail!");
                }
            }
        }
    }
}

// HostOwnedRelation
#[derive(Clone)]
struct HostOwnedRelation {
    global_entity: Option<GlobalEntity>,
}

impl HostOwnedRelation {
    pub fn new() -> Self {
        Self {
            global_entity: None,
        }
    }

    pub fn write(
        &self,
        writer: &mut dyn BitWrite,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) {
        if let Some(global_entity) = &self.global_entity {
            let Ok(local_entity) = converter.global_entity_to_local_entity(global_entity) else {
                warn!("Could not find Local Entity from Global Entity to associate with outgoing EntityProperty value!");
                return;
            };
            // Must reverse the LocalEntity because the Host<->Remote
            // relationship inverts after this data goes over the wire
            let reversed_local_entity = local_entity.to_reversed();

            true.ser(writer);
            reversed_local_entity.owned_ser(writer);
            return;
        }
        false.ser(writer);
    }

    pub fn bit_length(&self, converter: &dyn LocalEntityAndGlobalEntityConverter) -> u32 {
        let mut bit_counter = BitCounter::new(0, 0, u32::MAX);
        self.write(&mut bit_counter, converter);
        return bit_counter.bits_needed();
    }

    pub fn get<E: Copy + Eq + Hash>(
        &self,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
    ) -> Option<E> {
        if let Some(global_entity) = self.global_entity {
            if let Ok(world_entity) = converter.global_entity_to_entity(&global_entity) {
                return Some(world_entity);
            } else {
                warn!("Could not find World Entity from Global Entity, in order to get the EntityRelation value!");
                return None;
            }
        }
        return None;
    }

    pub fn set<E: Copy + Eq + Hash>(
        &mut self,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        world_entity: &E,
    ) {
        if let Ok(new_global_entity) = converter.entity_to_global_entity(world_entity) {
            self.global_entity = Some(new_global_entity);
        } else {
            warn!("Could not find Global Entity from World Entity, in order to set the EntityRelation value!");
            return;
        }
    }
}

// RemoteOwnedRelation
#[derive(Clone)]
struct RemoteOwnedRelation {
    global_entity: Option<GlobalEntity>,
}

impl RemoteOwnedRelation {
    fn new() -> Self {
        Self {
            global_entity: None,
        }
    }

    pub fn get<E: Copy + Eq + Hash>(
        &self,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
    ) -> Option<E> {
        if let Some(global_entity) = self.global_entity {
            if let Ok(world_entity) = converter.global_entity_to_entity(&global_entity) {
                return Some(world_entity);
            } else {
                warn!("Could not find World Entity from Global Entity, in order to get the EntityRelation value!");
                return None;
            }
        }
        return None;
    }
}

// RemoteWaitingRelation
#[derive(Clone)]
struct RemoteWaitingRelation {
    local_entity: LocalEntity,
}

impl RemoteWaitingRelation {
    fn new(local_entity: LocalEntity) -> Self {
        Self { local_entity }
    }
}
