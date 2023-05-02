use std::hash::Hash;

use log::warn;
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
    RemotePublic(RemotePublicRelation),
    Local(LocalRelation),
}

impl EntityRelation {
    fn is_host_owned(&self) -> bool {
        match self {
            EntityRelation::HostOwned(_) => true,
            _ => false,
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
    pub fn host_owned(mutator_index: u8) -> Self {
        Self {
            inner: EntityRelation::HostOwned(HostOwnedRelation::with_mutator(mutator_index)),
        }
    }

    // Read and create from Remote host
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

    pub fn set_mutator(&mut self, mutator: &PropertyMutator) {
        match &mut self.inner {
            EntityRelation::HostOwned(inner) => {
                inner.set_mutator(mutator);
            }
            EntityRelation::RemoteOwned(_)
            | EntityRelation::RemoteWaiting(_)
            | EntityRelation::RemotePublic(_) => {
                panic!("Remote EntityProperty should never call set_mutator()");
            }
            EntityRelation::Local(_) => {
                panic!("Local EntityProperty should never have a mutator.");
            }
        }
    }

    // Serialization / deserialization

    pub fn bit_length(&self, converter: &mut dyn LocalEntityAndGlobalEntityConverterMut) -> u32 {
        match &self.inner {
            EntityRelation::HostOwned(inner) => inner.bit_length(converter),
            EntityRelation::RemoteOwned(_) | EntityRelation::RemoteWaiting(_) => {
                panic!(
                    "Remote EntityProperty should never be written, so no need for their bit length."
                );
            }
            EntityRelation::RemotePublic(inner) => inner.bit_length(converter),
            EntityRelation::Local(_) => {
                panic!("Local Property should never be written, so no need for their bit length.");
            }
        }
    }

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
            EntityRelation::RemotePublic(inner) => {
                inner.write(writer, converter);
            }
            EntityRelation::Local(_) => {
                panic!("Local Property should never be written.");
            }
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
        let last_inner = std::mem::replace(&mut self.inner, new_inner);
        if let EntityRelation::RemotePublic(last_public_inner) = last_inner {
            let new_inner_copy = self.inner.clone();
            match new_inner_copy {
                EntityRelation::RemoteOwned(owned_inner) => {
                    let public_inner = owned_inner
                        .to_public(last_public_inner.mutator_index, &last_public_inner.mutator);
                    self.inner = EntityRelation::RemotePublic(public_inner);
                }
                EntityRelation::RemoteWaiting(waiting_inner) => {
                    self.inner = EntityRelation::RemoteWaiting(RemoteWaitingRelation::new_public(
                        waiting_inner.local_entity,
                        last_public_inner.mutator_index,
                        &last_public_inner.mutator,
                    ));
                }
                _ => {
                    panic!("RemotePublic EntityProperty should never read.");
                }
            }
        }
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
            EntityRelation::RemotePublic(inner) => inner.get(converter),
            EntityRelation::Local(inner) => inner.get(converter),
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
            EntityRelation::RemoteOwned(_)
            | EntityRelation::RemoteWaiting(_)
            | EntityRelation::RemotePublic(_) => {
                panic!("Remote EntityProperty should never be set manually.");
            }
            EntityRelation::Local(inner) => {
                inner.set(converter, entity);
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
                EntityRelation::RemotePublic(other_inner) => {
                    inner.mirror_public(other_inner);
                }
                EntityRelation::Local(other_inner) => {
                    inner.mirror_local(other_inner);
                }
            },
            EntityRelation::RemoteOwned(_)
            | EntityRelation::RemoteWaiting(_)
            | EntityRelation::RemotePublic(_) => {
                panic!("Remote EntityProperty should never be set manually.");
            }
            EntityRelation::Local(inner) => match &other.inner {
                EntityRelation::HostOwned(other_inner) => {
                    inner.mirror_host(other_inner);
                }
                EntityRelation::RemoteOwned(other_inner) => {
                    inner.mirror_remote(other_inner);
                }
                EntityRelation::RemoteWaiting(_) => {
                    inner.mirror_waiting();
                }
                EntityRelation::RemotePublic(other_inner) => {
                    inner.mirror_public(other_inner);
                }
                EntityRelation::Local(other_inner) => {
                    inner.mirror_local(other_inner);
                }
            },
        }
    }

    // Waiting

    pub fn waiting_local_entity(&self) -> Option<LocalEntity> {
        match &self.inner {
            EntityRelation::HostOwned(_)
            | EntityRelation::RemoteOwned(_)
            | EntityRelation::RemotePublic(_)
            | EntityRelation::Local(_) => None,
            EntityRelation::RemoteWaiting(inner) => Some(inner.local_entity),
        }
    }

    pub fn waiting_complete(&mut self, converter: &dyn LocalEntityAndGlobalEntityConverter) {
        match &mut self.inner {
            EntityRelation::HostOwned(_)
            | EntityRelation::RemoteOwned(_)
            | EntityRelation::RemotePublic(_)
            | EntityRelation::Local(_) => {
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
            EntityRelation::HostOwned(_)
            | EntityRelation::RemoteWaiting(_)
            | EntityRelation::Local(_) => {
                panic!("This type of EntityProperty can't use this method");
            }
            EntityRelation::RemoteOwned(inner) => {
                inner.write_local_entity(converter, writer);
            }
            EntityRelation::RemotePublic(inner) => {
                inner.write_local_entity(converter, writer);
            }
        }
    }

    /// Migrate Remote Property to Public version
    pub fn remote_publish(&mut self, mutator_index: u8, mutator: &PropertyMutator) {
        match &mut self.inner {
            EntityRelation::HostOwned(_) => {
                panic!("Host Relation should never be made public.");
            }
            EntityRelation::RemoteOwned(inner) => {
                let inner_value = inner.global_entity.clone();
                self.inner = EntityRelation::RemotePublic(RemotePublicRelation::new(
                    inner_value,
                    mutator_index,
                    mutator,
                ));
            }
            EntityRelation::RemotePublic(_) => {
                panic!("Remote Relation should never be made public twice.");
            }
            EntityRelation::Local(_) => {
                panic!("Local Relation should never be made public.");
            }
            EntityRelation::RemoteWaiting(inner) => {
                inner.remote_publish(mutator_index, mutator);
            }
        }
    }

    /// Migrate Host Property to Local version
    pub fn localize(&mut self) {
        match &mut self.inner {
            EntityRelation::HostOwned(inner) => {
                let inner_value = inner.global_entity.clone();
                self.inner = EntityRelation::Local(LocalRelation::new(inner_value));
            }
            EntityRelation::RemoteOwned(_)
            | EntityRelation::RemotePublic(_)
            | EntityRelation::RemoteWaiting(_) => {
                panic!("Remote Relation should never be made local.");
            }
            EntityRelation::Local(_) => {
                panic!("Local Relation should never be made local twice.");
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

    pub fn mirror_public(&mut self, other: &RemotePublicRelation) {
        self.global_entity = other.global_entity;
        self.mutate();
    }

    pub fn mirror_waiting(&mut self) {
        self.global_entity = None;
        self.mutate();
    }

    pub fn mirror_local(&mut self, other: &LocalRelation) {
        self.global_entity = other.global_entity;
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

    pub(crate) fn to_public(&self, index: u8, mutator: &PropertyMutator) -> RemotePublicRelation {
        RemotePublicRelation::new(self.global_entity.clone(), index, mutator)
    }
}

// RemoteWaitingRelation
#[derive(Clone)]
struct RemoteWaitingRelation {
    local_entity: LocalEntity,
    will_publish: Option<(u8, PropertyMutator)>,
}

impl RemoteWaitingRelation {
    fn new(local_entity: LocalEntity) -> Self {
        Self {
            local_entity,
            will_publish: None,
        }
    }
    fn new_public(local_entity: LocalEntity, index: u8, mutator: &PropertyMutator) -> Self {
        Self {
            local_entity,
            will_publish: Some((index, mutator.clone_new())),
        }
    }
    pub(crate) fn remote_publish(&mut self, index: u8, mutator: &PropertyMutator) {
        self.will_publish = Some((index, mutator.clone_new()));
    }
}

// RemoteOwnedRelation
#[derive(Clone)]
struct RemotePublicRelation {
    global_entity: Option<GlobalEntity>,
    mutator: PropertyMutator,
    mutator_index: u8,
}

impl RemotePublicRelation {
    pub fn new(
        global_entity: Option<GlobalEntity>,
        mutator_index: u8,
        mutator: &PropertyMutator,
    ) -> Self {
        Self {
            global_entity,
            mutator: mutator.clone_new(),
            mutator_index,
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

    pub fn bit_length(&self, converter: &mut dyn LocalEntityAndGlobalEntityConverterMut) -> u32 {
        let mut bit_counter = BitCounter::new(0, 0, u32::MAX);
        self.write(&mut bit_counter, converter);
        return bit_counter.bits_needed();
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
            false.ser(writer);
            return;
        };

        // Must reverse the LocalEntity because the Host<->Remote
        // relationship inverts after this data goes over the wire
        let reversed_local_entity = local_entity.to_reversed();

        true.ser(writer);
        reversed_local_entity.owned_ser(writer);
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

// LocalRelation
#[derive(Clone)]
struct LocalRelation {
    global_entity: Option<GlobalEntity>,
}

impl LocalRelation {
    pub fn new(global_entity: Option<GlobalEntity>) -> Self {
        Self { global_entity }
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

    pub fn mirror_host(&mut self, other: &HostOwnedRelation) {
        self.global_entity = other.global_entity;
    }

    pub fn mirror_remote(&mut self, other: &RemoteOwnedRelation) {
        self.global_entity = other.global_entity;
    }

    pub fn mirror_public(&mut self, other: &RemotePublicRelation) {
        self.global_entity = other.global_entity;
    }

    pub fn mirror_waiting(&mut self) {
        self.global_entity = None;
    }

    pub fn mirror_local(&mut self, other: &LocalRelation) {
        self.global_entity = other.global_entity;
    }
}
