use std::hash::Hash;

use log::{info, warn};
use naia_serde::{BitCounter, BitReader, BitWrite, BitWriter, Serde, SerdeErr};

use crate::{
    world::entity::{
        entity_converters::{
            LocalEntityAndGlobalEntityConverter,
            LocalEntityAndGlobalEntityConverterMut,
            EntityAndGlobalEntityConverter,
        },
        global_entity::GlobalEntity,
        local_entity::OwnedLocalEntity,
    },
    EntityAuthAccessor, PropertyMutator, RemoteEntity,
};

#[derive(Clone)]
enum EntityRelation {
    HostOwned(HostOwnedRelation),
    RemoteOwned(RemoteOwnedRelation),
    RemoteWaiting(RemoteWaitingRelation),
    RemotePublic(RemotePublicRelation),
    Delegated(DelegatedRelation),
    Local(LocalRelation),
    Invalid,
}

impl EntityRelation {
    fn clone_delegated(&self) -> Option<DelegatedRelation> {
        match self {
            EntityRelation::Delegated(inner) => Some(inner.clone()),
            _ => None,
        }
    }
    fn clone_public(&self) -> Option<RemotePublicRelation> {
        match self {
            EntityRelation::RemotePublic(inner) => Some(inner.clone()),
            _ => None,
        }
    }
    fn name(&self) -> &str {
        match self {
            EntityRelation::HostOwned(_) => "HostOwned",
            EntityRelation::RemoteOwned(_) => "RemoteOwned",
            EntityRelation::RemoteWaiting(_) => "RemoteWaiting",
            EntityRelation::RemotePublic(_) => "RemotePublic",
            EntityRelation::Delegated(_) => "Delegated",
            EntityRelation::Local(_) => "Local",
            EntityRelation::Invalid => "Invalid",
        }
    }
    fn write(
        &self,
        writer: &mut dyn BitWrite,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    ) {
        match self {
            EntityRelation::HostOwned(inner) => {
                inner.write(writer, converter);
            }
            EntityRelation::RemotePublic(inner) => {
                inner.write(writer, converter);
            }
            EntityRelation::Delegated(inner) => {
                inner.write(writer, converter);
            }
            EntityRelation::RemoteOwned(_)
            | EntityRelation::RemoteWaiting(_)
            | EntityRelation::Local(_)
            | EntityRelation::Invalid => {
                panic!(
                    "EntityProperty of inner type: `{:}` should never be written.",
                    self.name()
                );
            }
        }
    }
    fn set_mutator(&mut self, mutator: &PropertyMutator) {
        match self {
            EntityRelation::HostOwned(inner) => {
                inner.set_mutator(mutator);
            }
            EntityRelation::RemoteOwned(_)
            | EntityRelation::RemoteWaiting(_)
            | EntityRelation::RemotePublic(_)
            | EntityRelation::Local(_)
            | EntityRelation::Delegated(_)
            | EntityRelation::Invalid => {
                panic!(
                    "EntityProperty of inner type: `{:}` cannot call set_mutator()",
                    self.name()
                );
            }
        }
    }
    fn bit_length(&self, converter: &mut dyn LocalEntityAndGlobalEntityConverterMut) -> u32 {
        match self {
            EntityRelation::HostOwned(inner) => inner.bit_length(converter),
            EntityRelation::Delegated(inner) => inner.bit_length(converter),
            EntityRelation::RemotePublic(inner) => inner.bit_length(converter),
            EntityRelation::RemoteOwned(_)
            | EntityRelation::RemoteWaiting(_)
            | EntityRelation::Local(_)
            | EntityRelation::Invalid => {
                panic!(
                    "EntityProperty of inner type: `{:}` should never be written, so no need for their bit length.", self.name()
                );
            }
        }
    }
    fn get<E: Copy + Eq + Hash + Sync + Send>(
        &self,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
    ) -> Option<E> {
        let inner_global_entity = self.get_global_entity();

        if let Some(global_entity) = inner_global_entity {
            if let Ok(world_entity) = converter.global_entity_to_entity(&global_entity) {
                return Some(world_entity);
            } else {
                warn!("Could not find World Entity from Global Entity `{:?}`, in order to get the EntityRelation value!", global_entity);
                return None;
            }
        }
        warn!("Could not get EntityRelation value, because EntityRelation has no GlobalEntity!");
        return None;
    }

    fn set<E: Copy + Eq + Hash + Sync + Send>(
        &mut self,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        entity: &E,
    ) {
        match self {
            EntityRelation::HostOwned(inner) => {
                inner.set(converter, entity);
            }
            EntityRelation::Local(inner) => {
                inner.set(converter, entity);
            }
            EntityRelation::Delegated(inner) => {
                inner.set(converter, entity);
            }
            EntityRelation::RemoteOwned(_)
            | EntityRelation::RemoteWaiting(_)
            | EntityRelation::RemotePublic(_)
            | EntityRelation::Invalid => {
                panic!("Remote EntityProperty should never be set manually.");
            }
        }
    }
    fn set_to_none(&mut self) {
        match self {
            EntityRelation::HostOwned(inner) => {
                inner.set_to_none();
            }
            EntityRelation::Local(inner) => {
                inner.set_to_none();
            }
            EntityRelation::Delegated(inner) => {
                inner.set_to_none();
            }
            EntityRelation::RemoteOwned(_)
            | EntityRelation::RemoteWaiting(_)
            | EntityRelation::RemotePublic(_)
            | EntityRelation::Invalid => {
                panic!("Remote EntityProperty should never be set manually.");
            }
        }
    }
    fn mirror(&mut self, other: &EntityProperty) {
        match self {
            EntityRelation::HostOwned(inner) => match &other.inner {
                EntityRelation::HostOwned(other_inner) => {
                    inner.set_global_entity(&other_inner.global_entity);
                }
                EntityRelation::RemoteOwned(other_inner) => {
                    inner.set_global_entity(&other_inner.global_entity);
                }
                EntityRelation::RemotePublic(other_inner) => {
                    inner.set_global_entity(&other_inner.global_entity);
                }
                EntityRelation::Local(other_inner) => {
                    inner.set_global_entity(&other_inner.global_entity);
                }
                EntityRelation::Delegated(other_inner) => {
                    inner.set_global_entity(&other_inner.global_entity);
                }
                EntityRelation::RemoteWaiting(_) => {
                    inner.mirror_waiting();
                }
                EntityRelation::Invalid => {
                    panic!("Invalid EntityProperty should never be mirrored.");
                }
            },
            EntityRelation::Local(inner) => match &other.inner {
                EntityRelation::HostOwned(other_inner) => {
                    inner.set_global_entity(&other_inner.global_entity);
                }
                EntityRelation::RemoteOwned(other_inner) => {
                    inner.set_global_entity(&other_inner.global_entity);
                }
                EntityRelation::RemotePublic(other_inner) => {
                    inner.set_global_entity(&other_inner.global_entity);
                }
                EntityRelation::Local(other_inner) => {
                    inner.set_global_entity(&other_inner.global_entity);
                }
                EntityRelation::Delegated(other_inner) => {
                    inner.set_global_entity(&other_inner.global_entity);
                }
                EntityRelation::RemoteWaiting(_) => {
                    inner.mirror_waiting();
                }
                EntityRelation::Invalid => {
                    panic!("Invalid EntityProperty should never be mirrored.");
                }
            },
            EntityRelation::Delegated(inner) => match &other.inner {
                EntityRelation::HostOwned(other_inner) => {
                    inner.set_global_entity(&other_inner.global_entity);
                }
                EntityRelation::RemoteOwned(other_inner) => {
                    inner.set_global_entity(&other_inner.global_entity);
                }
                EntityRelation::RemotePublic(other_inner) => {
                    inner.set_global_entity(&other_inner.global_entity);
                }
                EntityRelation::Local(other_inner) => {
                    inner.set_global_entity(&other_inner.global_entity);
                }
                EntityRelation::Delegated(other_inner) => {
                    inner.set_global_entity(&other_inner.global_entity);
                }
                EntityRelation::RemoteWaiting(_) => {
                    inner.mirror_waiting();
                }
                EntityRelation::Invalid => {
                    panic!("Invalid EntityProperty should never be mirrored.");
                }
            },
            EntityRelation::RemoteOwned(_)
            | EntityRelation::RemoteWaiting(_)
            | EntityRelation::RemotePublic(_) => {
                panic!("Remote EntityProperty should never be set manually.");
            }
            EntityRelation::Invalid => {
                panic!("Invalid EntityProperty should never be set manually.");
            }
        }
    }
    fn waiting_local_entity(&self) -> Option<RemoteEntity> {
        match self {
            EntityRelation::HostOwned(_)
            | EntityRelation::RemoteOwned(_)
            | EntityRelation::RemotePublic(_)
            | EntityRelation::Local(_)
            | EntityRelation::Delegated(_)
            | EntityRelation::Invalid => None,
            EntityRelation::RemoteWaiting(inner) => Some(inner.remote_entity),
        }
    }
    pub fn write_local_entity(
        &self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        writer: &mut BitWriter,
    ) {
        match self {
            EntityRelation::RemoteOwned(inner) => {
                inner.write_local_entity(converter, writer);
            }
            EntityRelation::RemotePublic(inner) => {
                inner.write_local_entity(converter, writer);
            }
            EntityRelation::Delegated(inner) => {
                inner.write_local_entity(converter, writer);
            }
            EntityRelation::HostOwned(_)
            | EntityRelation::RemoteWaiting(_)
            | EntityRelation::Local(_)
            | EntityRelation::Invalid => {
                panic!(
                    "This type of EntityProperty: `{:?}` can't use this method",
                    self.name()
                );
            }
        }
    }

    fn get_global_entity(&self) -> Option<GlobalEntity> {
        match self {
            EntityRelation::HostOwned(inner) => inner.global_entity,
            EntityRelation::RemoteOwned(inner) => inner.global_entity,
            EntityRelation::RemotePublic(inner) => inner.global_entity,
            EntityRelation::Local(inner) => inner.global_entity,
            EntityRelation::Delegated(inner) => inner.global_entity,
            EntityRelation::RemoteWaiting(_) | EntityRelation::Invalid => None,
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
            // LocalEntity is reversed on write, don't worry here
            let local_entity = OwnedLocalEntity::de(reader)?;

            if let Ok(global_entity) = local_entity.convert_to_global(converter) {
                let mut new_impl = RemoteOwnedRelation::new_empty();
                new_impl.global_entity = Some(global_entity);

                let new_self = Self {
                    inner: EntityRelation::RemoteOwned(new_impl),
                };

                Ok(new_self)
            } else {
                if let OwnedLocalEntity::Remote(remote_entity_id) = local_entity {
                    let new_impl = RemoteWaitingRelation::new(RemoteEntity::new(remote_entity_id));

                    let new_self = Self {
                        inner: EntityRelation::RemoteWaiting(new_impl),
                    };

                    Ok(new_self)
                } else {
                    Ok(Self {
                        inner: EntityRelation::Invalid,
                    })
                }
            }
        } else {
            let mut new_impl = RemoteOwnedRelation::new_empty();
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
            OwnedLocalEntity::de(reader)?.ser(writer);
        }
        Ok(())
    }

    pub fn read(
        &mut self,
        reader: &mut BitReader,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) -> Result<(), SerdeErr> {
        let exists = bool::de(reader)?;
        let local_entity_opt = if exists {
            Some(OwnedLocalEntity::de(reader)?)
        } else {
            None
        };

        let eval = (
            self.inner.clone_public(),
            self.inner.clone_delegated(),
            local_entity_opt,
            local_entity_opt.map(|local_entity| local_entity.convert_to_global(converter)),
        );
        self.inner = match eval {
            (None, None, None, None) => {
                EntityRelation::RemoteOwned(RemoteOwnedRelation::new_empty())
            }
            (None, None, Some(local_entity), Some(Err(_))) => {
                info!("1 setting inner to RemoteWaiting");
                EntityRelation::RemoteWaiting(RemoteWaitingRelation::new(
                    local_entity.take_remote(),
                ))
            }
            (None, None, Some(_), Some(Ok(global_entity))) => EntityRelation::RemoteOwned(
                RemoteOwnedRelation::new_with_value(Some(global_entity)),
            ),
            (Some(public_relation), None, None, None) => EntityRelation::RemotePublic(
                RemotePublicRelation::new(None, public_relation.index, &public_relation.mutator),
            ),
            (Some(public_relation), None, Some(local_entity), Some(Err(_))) => {
                EntityRelation::RemoteWaiting(RemoteWaitingRelation::new_public(
                    local_entity.take_remote(),
                    public_relation.index,
                    &public_relation.mutator,
                ))
            }
            (Some(public_relation), None, Some(_), Some(Ok(global_entity))) => {
                EntityRelation::RemotePublic(RemotePublicRelation::new(
                    Some(global_entity),
                    public_relation.index,
                    &public_relation.mutator,
                ))
            }
            (None, Some(delegated_relation), None, None) => {
                EntityRelation::Delegated(delegated_relation.read_none())
            }
            (None, Some(delegated_relation), Some(local_entity), Some(Err(_))) => {
                info!("3 setting inner to RemoteWaiting");
                EntityRelation::RemoteWaiting(RemoteWaitingRelation::new_delegated(
                    local_entity.take_remote(),
                    &delegated_relation.auth_accessor,
                    &delegated_relation.mutator,
                    delegated_relation.index,
                ))
            }
            (None, Some(delegate_relation), Some(_), Some(Ok(global_entity))) => {
                EntityRelation::Delegated(delegate_relation.read_some(global_entity))
            }
            _ => {
                panic!("This shouldn't be possible. Unknown read case for EntityProperty.")
            }
        };

        Ok(())
    }

    pub fn waiting_complete(&mut self, converter: &dyn LocalEntityAndGlobalEntityConverter) {
        match &mut self.inner {
            EntityRelation::RemoteOwned(_)
            | EntityRelation::RemotePublic(_)
            | EntityRelation::Delegated(_) => {
                // already complete! this is intended behavior:
                // waiting Component/Message only sets EntityProperty to RemoteWaiting if it doesn't have an entity in-scope
                // but the entire Component/Message is put on the waitlist if even one of it's EntityProperties is RemoteWaiting
                // and `waiting_complete` is called on all of them, so we skip the already in-scope ones here
            }
            EntityRelation::RemoteWaiting(inner) => {
                let new_global_entity = {
                    if let Ok(global_entity) =
                        converter.remote_entity_to_global_entity(&inner.remote_entity)
                    {
                        Some(global_entity)
                    } else {
                        panic!("Error completing waiting EntityProperty! Could not convert RemoteEntity to GlobalEntity!");
                        // I hit this 2 times
                    }
                };

                if let Some((index, mutator)) = &inner.will_publish {
                    if let Some(accessor) = &inner.will_delegate {
                        // will publish and delegate
                        let mut new_impl =
                            DelegatedRelation::new(new_global_entity, accessor, mutator, *index);
                        new_impl.global_entity = new_global_entity;
                        self.inner = EntityRelation::Delegated(new_impl);
                    } else {
                        // will publish but not delegate
                        let new_impl =
                            RemotePublicRelation::new(new_global_entity, *index, mutator);
                        self.inner = EntityRelation::RemotePublic(new_impl);
                    }
                } else {
                    // will not publish or delegate
                    let mut new_impl = RemoteOwnedRelation::new_empty();
                    new_impl.global_entity = new_global_entity;
                    self.inner = EntityRelation::RemoteOwned(new_impl);
                }
            }
            EntityRelation::HostOwned(_) | EntityRelation::Local(_) | EntityRelation::Invalid => {
                panic!(
                    "Can't complete EntityProperty of type: `{:?}`!",
                    self.inner.name()
                );
            }
        }
    }

    /// Migrate Remote Property to Public version
    pub fn remote_publish(&mut self, mutator_index: u8, mutator: &PropertyMutator) {
        match &mut self.inner {
            EntityRelation::RemoteOwned(inner) => {
                let inner_value = inner.global_entity.clone();
                self.inner = EntityRelation::RemotePublic(RemotePublicRelation::new(
                    inner_value,
                    mutator_index,
                    mutator,
                ));
            }
            EntityRelation::RemoteWaiting(inner) => {
                inner.remote_publish(mutator_index, mutator);
            }
            EntityRelation::HostOwned(_)
            | EntityRelation::RemotePublic(_)
            | EntityRelation::Local(_)
            | EntityRelation::Delegated(_)
            | EntityRelation::Invalid => {
                panic!(
                    "EntityProperty of type: `{:?}` should never be made public twice.",
                    self.inner.name()
                );
            }
        }
    }

    /// Migrate Remote Property to Public version
    pub fn remote_unpublish(&mut self) {
        match &mut self.inner {
            EntityRelation::RemotePublic(inner) => {
                let inner_value = inner.global_entity.clone();
                self.inner = EntityRelation::RemoteOwned(RemoteOwnedRelation {
                    global_entity: inner_value,
                });
            }
            EntityRelation::RemoteWaiting(inner) => {
                inner.remote_unpublish();
            }
            EntityRelation::HostOwned(_)
            | EntityRelation::RemoteOwned(_)
            | EntityRelation::Local(_)
            | EntityRelation::Delegated(_)
            | EntityRelation::Invalid => {
                panic!(
                    "EntityProperty of type: `{:?}` should never be unpublished.",
                    self.inner.name()
                );
            }
        }
    }

    /// Migrate Host/RemotePublic Property to Delegated version
    pub fn enable_delegation(
        &mut self,
        accessor: &EntityAuthAccessor,
        mutator_opt: Option<(u8, &PropertyMutator)>,
    ) {
        let inner_value = self.inner.get_global_entity();

        let (mutator_index, mutator) = {
            if let Some((mutator_index, mutator)) = mutator_opt {
                // with mutator
                match &mut self.inner {
                    EntityRelation::RemoteOwned(_) => (mutator_index, mutator),
                    EntityRelation::RemoteWaiting(inner) => {
                        inner.remote_delegate(accessor);
                        return;
                    }
                    EntityRelation::Local(_)
                    | EntityRelation::RemotePublic(_)
                    | EntityRelation::HostOwned(_)
                    | EntityRelation::Delegated(_)
                    | EntityRelation::Invalid => {
                        panic!(
                            "EntityProperty of type `{:?}` should never enable delegation.",
                            self.inner.name()
                        );
                    }
                }
            } else {
                // without mutator
                match &mut self.inner {
                    EntityRelation::HostOwned(inner) => (
                        inner.index,
                        inner
                            .mutator
                            .as_ref()
                            .expect("should have a mutator by now"),
                    ),
                    EntityRelation::RemotePublic(inner) => (inner.index, &inner.mutator),
                    EntityRelation::Local(_)
                    | EntityRelation::RemoteOwned(_)
                    | EntityRelation::RemoteWaiting(_)
                    | EntityRelation::Delegated(_)
                    | EntityRelation::Invalid => {
                        panic!(
                            "EntityProperty of type `{:?}` should never enable delegation.",
                            self.inner.name()
                        );
                    }
                }
            }
        };

        self.inner = EntityRelation::Delegated(DelegatedRelation::new(
            inner_value,
            accessor,
            mutator,
            mutator_index,
        ));
    }

    /// Migrate Delegated Property to Host-Owned (Public) version
    pub fn disable_delegation(&mut self) {
        match &mut self.inner {
            EntityRelation::Delegated(inner) => {
                let inner_value = inner.global_entity.clone();
                let mut new_inner = HostOwnedRelation::with_mutator(inner.index);
                new_inner.set_mutator(&inner.mutator);
                new_inner.global_entity = inner_value;
                self.inner = EntityRelation::HostOwned(new_inner);
            }
            EntityRelation::RemoteWaiting(inner) => {
                inner.remote_undelegate();
            }
            EntityRelation::HostOwned(_)
            | EntityRelation::RemoteOwned(_)
            | EntityRelation::RemotePublic(_)
            | EntityRelation::Local(_)
            | EntityRelation::Invalid => {
                panic!(
                    "EntityProperty of type: `{:?}` should never disable delegation.",
                    self.inner.name()
                );
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
            EntityRelation::Delegated(inner) => {
                let inner_value = inner.global_entity.clone();
                self.inner = EntityRelation::Local(LocalRelation::new(inner_value));
            }
            EntityRelation::RemoteOwned(_)
            | EntityRelation::RemotePublic(_)
            | EntityRelation::RemoteWaiting(_)
            | EntityRelation::Local(_)
            | EntityRelation::Invalid => {
                panic!(
                    "EntityProperty of type: `{:?}` should never be made local.",
                    self.inner.name()
                );
            }
        }
    }

    // Pass-through

    pub fn set_mutator(&mut self, mutator: &PropertyMutator) {
        self.inner.set_mutator(mutator);
    }

    // Serialization / deserialization

    pub fn bit_length(&self, converter: &mut dyn LocalEntityAndGlobalEntityConverterMut) -> u32 {
        self.inner.bit_length(converter)
    }

    pub fn write(
        &self,
        writer: &mut dyn BitWrite,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    ) {
        self.inner.write(writer, converter);
    }

    pub fn get<E: Copy + Eq + Hash + Sync + Send>(
        &self,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
    ) -> Option<E> {
        self.inner.get(converter)
    }

    pub fn get_inner(&self) -> Option<GlobalEntity> {
        self.inner.get_global_entity()
    }

    pub fn set<E: Copy + Eq + Hash + Sync + Send>(
        &mut self,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        entity: &E,
    ) {
        self.inner.set(converter, entity);
    }

    pub fn set_to_none(&mut self) {
        self.inner.set_to_none();
    }

    pub fn mirror(&mut self, other: &EntityProperty) {
        self.inner.mirror(other);
    }

    pub fn waiting_local_entity(&self) -> Option<RemoteEntity> {
        self.inner.waiting_local_entity()
    }

    // used for writing out ready local entity value when splitting updates
    pub fn write_local_entity(
        &self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        writer: &mut BitWriter,
    ) {
        self.inner.write_local_entity(converter, writer);
    }
}

// HostOwnedRelation
#[derive(Clone)]
struct HostOwnedRelation {
    global_entity: Option<GlobalEntity>,
    mutator: Option<PropertyMutator>,
    index: u8,
}

impl HostOwnedRelation {
    pub fn new() -> Self {
        Self {
            global_entity: None,
            mutator: None,
            index: 0,
        }
    }

    pub fn with_mutator(mutate_index: u8) -> Self {
        Self {
            global_entity: None,
            mutator: None,
            index: mutate_index,
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
        let Ok(owned_local_entity) = converter.get_or_reserve_entity(global_entity) else {
            false.ser(writer);
            return;
        };

        // Must reverse the LocalEntity because the Host<->Remote
        // relationship inverts after this data goes over the wire
        let reversed_local_entity = owned_local_entity.to_reversed();

        true.ser(writer);
        reversed_local_entity.ser(writer);
    }

    pub fn bit_length(&self, converter: &mut dyn LocalEntityAndGlobalEntityConverterMut) -> u32 {
        let mut bit_counter = BitCounter::new(0, 0, u32::MAX);
        self.write(&mut bit_counter, converter);
        return bit_counter.bits_needed();
    }

    pub fn set<E: Copy + Eq + Hash + Sync + Send>(
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

    pub fn set_to_none(&mut self) {
        self.global_entity = None;
        self.mutate();
    }

    pub fn mirror_waiting(&mut self) {
        self.global_entity = None;
        self.mutate();
    }

    pub fn set_global_entity(&mut self, other_global_entity: &Option<GlobalEntity>) {
        self.global_entity = other_global_entity.clone();
        self.mutate();
    }

    fn mutate(&mut self) {
        let _success = if let Some(mutator) = &mut self.mutator {
            mutator.mutate(self.index)
        } else {
            false
        };
    }
}

// RemoteOwnedRelation
#[derive(Clone, Debug)]
struct RemoteOwnedRelation {
    global_entity: Option<GlobalEntity>,
}

impl RemoteOwnedRelation {
    fn new_empty() -> Self {
        Self {
            global_entity: None,
        }
    }

    fn new_with_value(global_entity: Option<GlobalEntity>) -> Self {
        Self { global_entity }
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
        let Ok(owned_entity) = converter.global_entity_to_owned_entity(&global_entity) else {
            warn!("Could not find Local Entity from Global Entity, in order to write the EntityRelation value! This should not happen.");
            false.ser(writer);
            return;
        };
        true.ser(writer);
        owned_entity.ser(writer);
    }
}

// RemoteWaitingRelation
#[derive(Clone)]
struct RemoteWaitingRelation {
    remote_entity: RemoteEntity,
    will_publish: Option<(u8, PropertyMutator)>,
    will_delegate: Option<EntityAuthAccessor>,
}

impl RemoteWaitingRelation {
    fn new(remote_entity: RemoteEntity) -> Self {
        Self {
            remote_entity,
            will_publish: None,
            will_delegate: None,
        }
    }
    fn new_public(remote_entity: RemoteEntity, index: u8, mutator: &PropertyMutator) -> Self {
        Self {
            remote_entity,
            will_publish: Some((index, mutator.clone_new())),
            will_delegate: None,
        }
    }
    fn new_delegated(
        local_entity: RemoteEntity,
        auth_accessor: &EntityAuthAccessor,
        mutator: &PropertyMutator,
        index: u8,
    ) -> Self {
        Self {
            remote_entity: local_entity,
            will_publish: Some((index, mutator.clone_new())),
            will_delegate: Some(auth_accessor.clone()),
        }
    }
    pub(crate) fn remote_publish(&mut self, index: u8, mutator: &PropertyMutator) {
        self.will_publish = Some((index, mutator.clone_new()));
    }
    pub(crate) fn remote_unpublish(&mut self) {
        self.will_publish = None;
    }
    pub(crate) fn remote_delegate(&mut self, accessor: &EntityAuthAccessor) {
        self.will_delegate = Some(accessor.clone());
    }
    pub(crate) fn remote_undelegate(&mut self) {
        self.will_delegate = None;
    }
}

// RemoteOwnedRelation
#[derive(Clone)]
struct RemotePublicRelation {
    global_entity: Option<GlobalEntity>,
    mutator: PropertyMutator,
    index: u8,
}

impl RemotePublicRelation {
    pub fn new(global_entity: Option<GlobalEntity>, index: u8, mutator: &PropertyMutator) -> Self {
        Self {
            global_entity,
            mutator: mutator.clone_new(),
            index,
        }
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
        let Ok(local_entity) = converter.get_or_reserve_entity(global_entity) else {
            false.ser(writer);
            return;
        };

        // Must reverse the LocalEntity because the Host<->Remote
        // relationship inverts after this data goes over the wire
        let reversed_local_entity = local_entity.to_reversed();

        true.ser(writer);
        reversed_local_entity.ser(writer);
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
        let Ok(owned_entity) = converter.global_entity_to_owned_entity(&global_entity) else {
            warn!("Could not find Local Entity from Global Entity, in order to write the EntityRelation value! This should not happen.");
            false.ser(writer);
            return;
        };
        true.ser(writer);
        owned_entity.ser(writer);
    }
}

// DelegatedRelation
#[derive(Clone)]
struct DelegatedRelation {
    global_entity: Option<GlobalEntity>,
    auth_accessor: EntityAuthAccessor,
    mutator: PropertyMutator,
    index: u8,
}

impl DelegatedRelation {
    /// Create a new DelegatedRelation
    pub fn new(
        global_entity: Option<GlobalEntity>,
        auth_accessor: &EntityAuthAccessor,
        mutator: &PropertyMutator,
        index: u8,
    ) -> Self {
        Self {
            global_entity,
            auth_accessor: auth_accessor.clone(),
            mutator: mutator.clone_new(),
            index,
        }
    }

    pub fn set<E: Copy + Eq + Hash + Sync + Send>(
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

    pub fn set_to_none(&mut self) {
        self.global_entity = None;
        self.mutate();
    }

    pub fn set_global_entity(&mut self, other_global_entity: &Option<GlobalEntity>) {
        self.global_entity = other_global_entity.clone();
        self.mutate();
    }

    pub fn mirror_waiting(&mut self) {
        self.global_entity = None;
        self.mutate();
    }

    pub fn read_none(mut self) -> Self {
        if self.can_read() {
            self.global_entity = None;
            self.mutate();
        }

        self
    }

    pub fn read_some(mut self, global_entity: GlobalEntity) -> Self {
        if self.can_read() {
            self.global_entity = Some(global_entity);
            self.mutate();
        }

        self
    }

    pub fn bit_length(&self, converter: &mut dyn LocalEntityAndGlobalEntityConverterMut) -> u32 {
        if !self.can_write() {
            panic!("Must have Authority over Entity before performing this operation.");
        }
        let mut bit_counter = BitCounter::new(0, 0, u32::MAX);
        self.write(&mut bit_counter, converter);
        return bit_counter.bits_needed();
    }

    pub fn write(
        &self,
        writer: &mut dyn BitWrite,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    ) {
        if !self.can_write() {
            panic!("Must have Authority over Entity before performing this operation.");
        }

        let Some(global_entity) = &self.global_entity else {
            false.ser(writer);
            return;
        };
        let Ok(local_entity) = converter.get_or_reserve_entity(global_entity) else {
            false.ser(writer);
            return;
        };

        // Must reverse the LocalEntity because the Host<->Remote
        // relationship inverts after this data goes over the wire
        let reversed_local_entity = local_entity.to_reversed();

        true.ser(writer);
        reversed_local_entity.ser(writer);
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
        let Ok(host_entity) = converter.global_entity_to_owned_entity(&global_entity) else {
            warn!("Could not find Local Entity from Global Entity, in order to write the EntityRelation value! This should not happen.");
            false.ser(writer);
            return;
        };
        true.ser(writer);
        host_entity.ser(writer);
    }

    fn mutate(&mut self) {
        if !self.can_mutate() {
            panic!("Must request authority to mutate a Delegated EntityProperty.");
        }
        let _success = self.mutator.mutate(self.index);
    }

    fn can_mutate(&self) -> bool {
        self.auth_accessor.auth_status().can_mutate()
    }

    fn can_read(&self) -> bool {
        self.auth_accessor.auth_status().can_read()
    }

    fn can_write(&self) -> bool {
        self.auth_accessor.auth_status().can_write()
    }
}

// LocalRelation
#[derive(Clone, Debug)]
struct LocalRelation {
    global_entity: Option<GlobalEntity>,
}

impl LocalRelation {
    pub fn new(global_entity: Option<GlobalEntity>) -> Self {
        Self { global_entity }
    }

    pub fn set<E: Copy + Eq + Hash + Sync + Send>(
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

    pub fn set_to_none(&mut self) {
        self.global_entity = None;
    }

    pub fn mirror_waiting(&mut self) {
        self.global_entity = None;
    }

    pub fn set_global_entity(&mut self, other_global_entity: &Option<GlobalEntity>) {
        self.global_entity = other_global_entity.clone();
    }
}
