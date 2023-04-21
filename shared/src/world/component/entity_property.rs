use log::warn;
use std::hash::Hash;

use naia_serde::{BitCounter, BitReader, BitWrite, BitWriter, Serde, SerdeErr};

use crate::{
    world::entity::{
        entity_converters::{
            EntityAndGlobalEntityConverter, LocalEntityAndGlobalEntityConverter,
            LocalEntityAndGlobalEntityConverterMut,
        },
        global_entity::GlobalEntity,
        local_entity::LocalEntity,
    },
    PropertyMutator,
};

#[derive(Clone)]
enum EntityRelation {
    HostOwned(HostOwnedRelation),
    RemoteOwned(RemoteOwnedRelation),
    RemoteWaiting(RemoteWaitingRelation),
}

impl EntityRelation {
    fn is_host_owned(&self) -> bool {
        match self {
            EntityRelation::HostOwned(_) => true,
            EntityRelation::RemoteOwned(_) | EntityRelation::RemoteWaiting(_) => false,
        }
    }
}

#[derive(Clone)]
pub struct EntityProperty {
    inner: EntityRelation,
}

impl EntityProperty {
    // Should only be used by Messages
    pub fn new() -> Self {
        Self {
            inner: EntityRelation::HostOwned(HostOwnedRelation::new()),
        }
    }

    // Should only be used by Components
    pub fn with_mutator(mutator_index: u8) -> Self {
        Self {
            inner: EntityRelation::HostOwned(HostOwnedRelation::with_mutator(mutator_index)),
        }
    }

    pub fn set_mutator(&mut self, mutator: &PropertyMutator) {
        match &mut self.inner {
            EntityRelation::HostOwned(inner) => {
                inner.set_mutator(mutator);
            }
            EntityRelation::RemoteOwned(_) | EntityRelation::RemoteWaiting(_) => {
                panic!("Remote EntityProperty should never have a mutator.");
            }
        }
    }

    // Serialization / deserialization

    pub fn write(
        &self,
        writer: &mut dyn BitWrite,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    ) {
        match &self.inner {
            EntityRelation::HostOwned(inner) => {
                inner.write(writer, converter);
            }
            EntityRelation::RemoteOwned(_) | EntityRelation::RemoteWaiting(_) => {
                panic!("Remote EntityProperty should never be written.");
            }
        }
    }

    pub fn bit_length(&self, converter: &mut dyn LocalEntityAndGlobalEntityConverterMut) -> u32 {
        match &self.inner {
            EntityRelation::HostOwned(inner) => inner.bit_length(converter),
            EntityRelation::RemoteOwned(_) | EntityRelation::RemoteWaiting(_) => {
                panic!(
                    "Remote EntityProperty should never be written, so no need for their bit length."
                );
            }
        }
    }

    pub fn new_read(
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
                    inner: EntityRelation::RemoteOwned(new_impl),
                };

                Ok(new_self)
            } else {
                let new_impl = RemoteWaitingRelation::new(local_entity);

                let new_self = Self {
                    inner: EntityRelation::RemoteWaiting(new_impl),
                };

                Ok(new_self)
            }
        } else {
            let mut new_impl = RemoteOwnedRelation::new();
            new_impl.global_entity = None;

            let new_self = Self {
                inner: EntityRelation::RemoteOwned(new_impl),
            };

            Ok(new_self)
        }
    }

    pub fn read_write(reader: &mut BitReader, writer: &mut BitWriter) -> Result<(), SerdeErr> {
        let exists = bool::de(reader)?;
        exists.ser(writer);
        if exists {
            LocalEntity::owned_de(reader)?.owned_ser(writer);
        }
        Ok(())
    }

    pub fn read(
        &mut self,
        reader: &mut BitReader,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) -> Result<(), SerdeErr> {
        if self.inner.is_host_owned() {
            panic!("HostOwned EntityProperty should never read.");
        }
        let exists = bool::de(reader)?;
        let new_inner = {
            if exists {
                let local_entity = LocalEntity::owned_de(reader)?;
                if let Ok(global_entity) = converter.local_entity_to_global_entity(&local_entity) {
                    let mut new_impl = RemoteOwnedRelation::new();
                    new_impl.global_entity = Some(global_entity);
                    EntityRelation::RemoteOwned(new_impl)
                } else {
                    let new_impl = RemoteWaitingRelation::new(local_entity);
                    EntityRelation::RemoteWaiting(new_impl)
                }
            } else {
                let mut new_impl = RemoteOwnedRelation::new();
                new_impl.global_entity = None;
                EntityRelation::RemoteOwned(new_impl)
            }
        };
        self.inner = new_inner;
        Ok(())
    }

    // Internal

    pub fn get<E: Copy + Eq + Hash>(
        &self,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
    ) -> Option<E> {
        match &self.inner {
            EntityRelation::HostOwned(inner) => inner.get(converter),
            EntityRelation::RemoteOwned(inner) => inner.get(converter),
            EntityRelation::RemoteWaiting(_) => {
                panic!("Not ready to get RemoteWaiting EntityProperty value!");
            }
        }
    }

    pub fn set<E: Copy + Eq + Hash>(
        &mut self,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        entity: &E,
    ) {
        match &mut self.inner {
            EntityRelation::HostOwned(inner) => {
                inner.set(converter, entity);
            }
            EntityRelation::RemoteOwned(_) | EntityRelation::RemoteWaiting(_) => {
                panic!("Remote EntityProperty should never be set manually.");
            }
        }
    }

    pub fn mirror(&mut self, other: &EntityProperty) {
        match &mut self.inner {
            EntityRelation::HostOwned(inner) => match &other.inner {
                EntityRelation::HostOwned(other_inner) => {
                    inner.mirror_host(other_inner);
                }
                EntityRelation::RemoteOwned(other_inner) => {
                    inner.mirror_remote(other_inner);
                }
                EntityRelation::RemoteWaiting(_) => {
                    inner.mirror_waiting();
                }
            },
            EntityRelation::RemoteOwned(_) | EntityRelation::RemoteWaiting(_) => {
                panic!("Remote EntityProperty should never be set manually.");
            }
        }
    }

    // Waiting

    pub fn waiting_local_entity(&self) -> Option<LocalEntity> {
        match &self.inner {
            EntityRelation::HostOwned(_) | EntityRelation::RemoteOwned(_) => None,
            EntityRelation::RemoteWaiting(inner) => Some(inner.local_entity),
        }
    }

    pub fn waiting_complete(&mut self, converter: &dyn LocalEntityAndGlobalEntityConverter) {
        match &mut self.inner {
            EntityRelation::HostOwned(_) | EntityRelation::RemoteOwned(_) => {
                panic!("Can't complete a RemoteOwned or HostOwned Relation!");
            }
            EntityRelation::RemoteWaiting(inner) => {
                if let Ok(global_entity) =
                    converter.local_entity_to_global_entity(&inner.local_entity)
                {
                    let mut new_impl = RemoteOwnedRelation::new();
                    new_impl.global_entity = Some(global_entity);

                    self.inner = EntityRelation::RemoteOwned(new_impl);
                } else {
                    panic!("Could not find Global Entity from Local Entity! Should only call `waiting_complete` when it is known the converter will not fail!");
                }
            }
        }
    }

    // used for writing out ready local entity value when splitting updates
    pub fn write_local_entity(
        &self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        writer: &mut BitWriter,
    ) {
        match &self.inner {
            EntityRelation::HostOwned(_) | EntityRelation::RemoteWaiting(_) => {
                panic!("Can't use this method to write a RemoteWaiting or HostOwned Relation!");
            }
            EntityRelation::RemoteOwned(inner) => {
                inner.write_local_entity(converter, writer);
            }
        }
    }
}

// HostOwnedRelation
#[derive(Clone)]
struct HostOwnedRelation {
    global_entity: Option<GlobalEntity>,
    mutator: Option<PropertyMutator>,
    mutator_index: u8,
}

impl HostOwnedRelation {
    pub fn new() -> Self {
        Self {
            global_entity: None,
            mutator: None,
            mutator_index: 0,
        }
    }

    pub fn with_mutator(mutate_index: u8) -> Self {
        Self {
            global_entity: None,
            mutator: None,
            mutator_index: mutate_index,
        }
    }

    pub fn set_mutator(&mut self, mutator: &PropertyMutator) {
        self.mutator = Some(mutator.clone_new());
    }

    pub fn write(
        &self,
        writer: &mut dyn BitWrite,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    ) {
        let Some(global_entity) = &self.global_entity else {
            false.ser(writer);
            return;
        };
        let Ok(local_entity) = converter.get_or_reserve_host_entity(global_entity) else {
            warn!("Global Entity does not Exist! This should not happen.");
            false.ser(writer);
            return;
        };

        // Must reverse the LocalEntity because the Host<->Remote
        // relationship inverts after this data goes over the wire
        let reversed_local_entity = local_entity.to_reversed();

        true.ser(writer);
        reversed_local_entity.owned_ser(writer);
    }

    pub fn bit_length(&self, converter: &mut dyn LocalEntityAndGlobalEntityConverterMut) -> u32 {
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
            self.mutate();
        } else {
            warn!("Could not find Global Entity from World Entity, in order to set the EntityRelation value!");
            return;
        }
    }

    pub fn mirror_host(&mut self, other: &HostOwnedRelation) {
        self.global_entity = other.global_entity;
        self.mutate();
    }

    pub fn mirror_remote(&mut self, other: &RemoteOwnedRelation) {
        self.global_entity = other.global_entity;
        self.mutate();
    }

    pub fn mirror_waiting(&mut self) {
        self.global_entity = None;
        self.mutate();
    }

    fn mutate(&mut self) {
        if let Some(mutator) = &mut self.mutator {
            mutator.mutate(self.mutator_index);
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

    pub fn write_local_entity(
        &self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        writer: &mut BitWriter,
    ) {
        let Some(global_entity) = &self.global_entity else {
            false.ser(writer);
            return;
        };
        let Ok(local_entity) = converter.global_entity_to_local_entity(&global_entity) else {
            warn!("Could not find Local Entity from Global Entity, in order to write the EntityRelation value! This should not happen.");
            false.ser(writer);
            return;
        };
        true.ser(writer);
        local_entity.owned_ser(writer);
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
