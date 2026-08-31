use std::hash::Hash;

use log::{info, warn};
use naia_serde::{BitCounter, BitReader, BitWrite, BitWriter, Serde, SerdeErr};

use crate::world::local::local_entity::OwnedLocalEntity;
use crate::{
    world::entity::{
        entity_converters::{
            EntityAndGlobalEntityConverter, LocalEntityAndGlobalEntityConverter,
            LocalEntityAndGlobalEntityConverterMut,
        },
        global_entity::GlobalEntity,
    },
    EntityAuthAccessor, PropertyMutator, RemoteEntity,
};

#[derive(Clone)]
enum EntityRelation {
    HostCreated(HostCreatedRelation),
    RemoteCreated(RemoteCreatedRelation),
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
            EntityRelation::HostCreated(_) => "HostOwned",
            EntityRelation::RemoteCreated(_) => "RemoteOwned",
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
            EntityRelation::HostCreated(inner) => {
                inner.write(writer, converter);
            }
            EntityRelation::RemotePublic(inner) => {
                inner.write(writer, converter);
            }
            EntityRelation::Delegated(inner) => {
                inner.write(writer, converter);
            }
            EntityRelation::RemoteCreated(_)
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
            EntityRelation::HostCreated(inner) => {
                inner.set_mutator(mutator);
            }
            EntityRelation::RemoteCreated(_)
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
            EntityRelation::HostCreated(inner) => inner.bit_length(converter),
            EntityRelation::Delegated(inner) => inner.bit_length(converter),
            EntityRelation::RemotePublic(inner) => inner.bit_length(converter),
            EntityRelation::RemoteCreated(_)
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
        None
    }

    fn set<E: Copy + Eq + Hash + Sync + Send>(
        &mut self,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        entity: &E,
    ) {
        match self {
            EntityRelation::HostCreated(inner) => {
                inner.set(converter, entity);
            }
            EntityRelation::Local(inner) => {
                inner.set(converter, entity);
            }
            EntityRelation::Delegated(inner) => {
                inner.set(converter, entity);
            }
            EntityRelation::RemoteCreated(_)
            | EntityRelation::RemoteWaiting(_)
            | EntityRelation::RemotePublic(_)
            | EntityRelation::Invalid => {
                panic!("Remote EntityProperty should never be set manually.");
            }
        }
    }
    fn set_to_none(&mut self) {
        match self {
            EntityRelation::HostCreated(inner) => {
                inner.set_to_none();
            }
            EntityRelation::Local(inner) => {
                inner.set_to_none();
            }
            EntityRelation::Delegated(inner) => {
                inner.set_to_none();
            }
            EntityRelation::RemoteCreated(_)
            | EntityRelation::RemoteWaiting(_)
            | EntityRelation::RemotePublic(_)
            | EntityRelation::Invalid => {
                panic!("Remote EntityProperty should never be set manually.");
            }
        }
    }
    fn mirror(&mut self, other: &EntityProperty) {
        match self {
            EntityRelation::HostCreated(inner) => match &other.inner {
                EntityRelation::HostCreated(other_inner) => {
                    inner.set_global_entity(&other_inner.global_entity);
                }
                EntityRelation::RemoteCreated(other_inner) => {
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
                EntityRelation::HostCreated(other_inner) => {
                    inner.set_global_entity(&other_inner.global_entity);
                }
                EntityRelation::RemoteCreated(other_inner) => {
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
                EntityRelation::HostCreated(other_inner) => {
                    inner.set_global_entity(&other_inner.global_entity);
                }
                EntityRelation::RemoteCreated(other_inner) => {
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
            EntityRelation::RemoteCreated(_)
            | EntityRelation::RemoteWaiting(_)
            | EntityRelation::RemotePublic(_) => {
                panic!("Remote EntityProperty should never be set manually.");
            }
            EntityRelation::Invalid => {
                panic!("Invalid EntityProperty should never be set manually.");
            }
        }
    }
    fn waiting_remote_entity(&self) -> Option<RemoteEntity> {
        match self {
            EntityRelation::HostCreated(_)
            | EntityRelation::RemoteCreated(_)
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
            EntityRelation::RemoteCreated(inner) => {
                inner.write_local_entity(converter, writer);
            }
            EntityRelation::RemotePublic(inner) => {
                inner.write_local_entity(converter, writer);
            }
            EntityRelation::Delegated(inner) => {
                inner.write_local_entity(converter, writer);
            }
            EntityRelation::HostCreated(_)
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
            EntityRelation::HostCreated(inner) => inner.global_entity,
            EntityRelation::RemoteCreated(inner) => inner.global_entity,
            EntityRelation::RemotePublic(inner) => inner.global_entity,
            EntityRelation::Local(inner) => inner.global_entity,
            EntityRelation::Delegated(inner) => inner.global_entity,
            EntityRelation::RemoteWaiting(_) | EntityRelation::Invalid => None,
        }
    }
}

/// A component field that stores an optional reference to another entity, with lifecycle tracking across host/remote/delegated states.
#[derive(Clone)]
pub struct EntityProperty {
    inner: EntityRelation,
}

impl EntityProperty {
    /// Creates an `EntityProperty` initialized for use inside a `Message` (no mutator).
    // Should only be used by Messages
    pub fn new_for_message() -> Self {
        Self {
            inner: EntityRelation::HostCreated(HostCreatedRelation::new()),
        }
    }

    /// Creates an `EntityProperty` initialized for use inside a `Component` at the given property index.
    // Should only be used by Components
    pub fn new_for_component(mutator_index: u8) -> Self {
        Self {
            inner: EntityRelation::HostCreated(HostCreatedRelation::with_mutator(mutator_index)),
        }
    }

    /// Deserializes a new `EntityProperty` from the remote host's bit stream.
    // Read and create from Remote host
    pub fn new_read(
        reader: &mut BitReader,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) -> Result<Self, SerdeErr> {
        let exists = bool::de(reader)?;
        if exists {
            // LocalEntity is reversed on write, don't worry here
            let local_entity = OwnedLocalEntity::de(reader)?;

            // CRITICAL: Apply entity redirects for migrated entities
            // If an entity was migrated (e.g., RemoteEntity → HostEntity), the EntityProperty
            // might reference the old entity ID. The redirect system ensures we use the new ID.
            let redirected_entity = converter.apply_entity_redirect(&local_entity);

            // info!("EntityProperty::new_read() local_entity: {:?}, redirected: {:?}", local_entity, redirected_entity);

            if let Ok(global_entity) = redirected_entity.convert_to_global(converter) {
                let mut new_impl = RemoteCreatedRelation::new_empty();
                new_impl.global_entity = Some(global_entity);

                let new_self = Self {
                    inner: EntityRelation::RemoteCreated(new_impl),
                };

                Ok(new_self)
            } else if let OwnedLocalEntity::Remote { .. } = redirected_entity {
                let new_impl = RemoteWaitingRelation::new(redirected_entity.take_remote());

                let new_self = Self {
                    inner: EntityRelation::RemoteWaiting(new_impl),
                };

                Ok(new_self)
            } else {
                Ok(Self {
                    inner: EntityRelation::Invalid,
                })
            }
        } else {
            let mut new_impl = RemoteCreatedRelation::new_empty();
            new_impl.global_entity = None;

            let new_self = Self {
                inner: EntityRelation::RemoteCreated(new_impl),
            };

            Ok(new_self)
        }
    }

    /// Passes through an entity-property bit field from `reader` to `writer` without resolving entities.
    pub fn read_write(reader: &mut BitReader, writer: &mut BitWriter) -> Result<(), SerdeErr> {
        let exists = bool::de(reader)?;
        exists.ser(writer);
        if exists {
            OwnedLocalEntity::de(reader)?.ser(writer);
        }
        Ok(())
    }

    /// Updates this property's inner relation from the remote host's bit stream.
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
                EntityRelation::RemoteCreated(RemoteCreatedRelation::new_empty())
            }
            (None, None, Some(local_entity), Some(Err(_))) => {
                info!("1 setting inner to RemoteWaiting");
                EntityRelation::RemoteWaiting(RemoteWaitingRelation::new(
                    local_entity.take_remote(),
                ))
            }
            (None, None, Some(_), Some(Ok(global_entity))) => EntityRelation::RemoteCreated(
                RemoteCreatedRelation::new_with_value(Some(global_entity)),
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
                // Unreachable, and so untestable: `inner` is a single variant,
                // so `clone_public()` and `clone_delegated()` can never both be
                // `Some`, and the third and fourth tuple slots are derived from
                // the same `local_entity_opt` (both `None` or both `Some`).
                // Every remaining combination is enumerated above.
                panic!("This shouldn't be possible. Unknown read case for EntityProperty.")
            }
        };

        Ok(())
    }

    /// Resolves a waiting entity relation now that its target entity has arrived.
    pub fn waiting_complete(&mut self, converter: &dyn LocalEntityAndGlobalEntityConverter) {
        match &mut self.inner {
            EntityRelation::RemoteCreated(_)
            | EntityRelation::RemotePublic(_)
            | EntityRelation::Delegated(_) => {
                // already complete! this is intended behavior:
                // waiting Component/Message only sets EntityProperty to RemoteWaiting if it doesn't have an entity in-scope
                // but the entire Component/Message is put on the waitlist if even one of it's EntityProperties is RemoteWaiting
                // and `waiting_complete` is called on all of them, so we skip the already in-scope ones here
            }
            EntityRelation::RemoteWaiting(inner) => {
                let new_global_entity = {
                    // CRITICAL: Apply entity redirects for migrated entities
                    // The RemoteEntity stored here might reference an old entity ID before migration
                    let owned_entity = inner.remote_entity.copy_to_owned();
                    let redirected_entity = converter.apply_entity_redirect(&owned_entity);

                    if let Ok(global_entity) = redirected_entity.convert_to_global(converter) {
                        Some(global_entity)
                    } else {
                        panic!("Error completing waiting EntityProperty! Could not convert RemoteEntity to GlobalEntity! Original: {:?}, Redirected: {:?}", 
                               owned_entity, redirected_entity);
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
                    let mut new_impl = RemoteCreatedRelation::new_empty();
                    new_impl.global_entity = new_global_entity;
                    self.inner = EntityRelation::RemoteCreated(new_impl);
                }
            }
            EntityRelation::HostCreated(_) | EntityRelation::Local(_) | EntityRelation::Invalid => {
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
            EntityRelation::RemoteCreated(inner) => {
                let inner_value = inner.global_entity;
                self.inner = EntityRelation::RemotePublic(RemotePublicRelation::new(
                    inner_value,
                    mutator_index,
                    mutator,
                ));
            }
            EntityRelation::RemoteWaiting(inner) => {
                inner.remote_publish(mutator_index, mutator);
            }
            EntityRelation::HostCreated(_)
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
                let inner_value = inner.global_entity;
                self.inner = EntityRelation::RemoteCreated(RemoteCreatedRelation {
                    global_entity: inner_value,
                });
            }
            EntityRelation::RemoteWaiting(inner) => {
                inner.remote_unpublish();
            }
            EntityRelation::HostCreated(_)
            | EntityRelation::RemoteCreated(_)
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
                    EntityRelation::RemoteCreated(_) => (mutator_index, mutator),
                    EntityRelation::RemoteWaiting(inner) => {
                        inner.remote_delegate(accessor);
                        return;
                    }
                    EntityRelation::Local(_)
                    | EntityRelation::RemotePublic(_)
                    | EntityRelation::HostCreated(_)
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
                    EntityRelation::HostCreated(inner) => (
                        inner.index,
                        inner
                            .mutator
                            .as_ref()
                            .expect("should have a mutator by now"),
                    ),
                    EntityRelation::RemotePublic(inner) => (inner.index, &inner.mutator),
                    EntityRelation::Local(_)
                    | EntityRelation::RemoteCreated(_)
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
                let inner_value = inner.global_entity;
                let mut new_inner = HostCreatedRelation::with_mutator(inner.index);
                new_inner.set_mutator(&inner.mutator);
                new_inner.global_entity = inner_value;
                self.inner = EntityRelation::HostCreated(new_inner);
            }
            EntityRelation::RemoteWaiting(inner) => {
                inner.remote_undelegate();
            }
            EntityRelation::HostCreated(_)
            | EntityRelation::RemoteCreated(_)
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
            EntityRelation::HostCreated(inner) => {
                let inner_value = inner.global_entity;
                self.inner = EntityRelation::Local(LocalRelation::new(inner_value));
            }
            EntityRelation::Delegated(inner) => {
                let inner_value = inner.global_entity;
                self.inner = EntityRelation::Local(LocalRelation::new(inner_value));
            }
            EntityRelation::RemoteCreated(_)
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

    /// Sets the property mutator used to mark this field dirty on value changes.
    pub fn set_mutator(&mut self, mutator: &PropertyMutator) {
        self.inner.set_mutator(mutator);
    }

    // Serialization / deserialization

    /// Returns the serialized bit length of this property given `converter`.
    pub fn bit_length(&self, converter: &mut dyn LocalEntityAndGlobalEntityConverterMut) -> u32 {
        self.inner.bit_length(converter)
    }

    /// Writes this property's entity reference bits into `writer`.
    pub fn write(
        &self,
        writer: &mut dyn BitWrite,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    ) {
        self.inner.write(writer, converter);
    }

    /// Returns the world entity referenced by this property, translated via `converter`, or `None`.
    pub fn get<E: Copy + Eq + Hash + Sync + Send>(
        &self,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
    ) -> Option<E> {
        self.inner.get(converter)
    }

    /// Returns the raw `GlobalEntity` stored in this property, or `None`.
    pub fn get_inner(&self) -> Option<GlobalEntity> {
        self.inner.get_global_entity()
    }

    /// Sets this property to point at `entity`, converting it to a `GlobalEntity` via `converter`.
    pub fn set<E: Copy + Eq + Hash + Sync + Send>(
        &mut self,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        entity: &E,
    ) {
        self.inner.set(converter, entity);
    }

    /// Clears this property so it no longer references any entity.
    pub fn set_to_none(&mut self) {
        self.inner.set_to_none();
    }

    /// Copies the referenced entity from `other` into `self`, preserving `self`'s relation type.
    pub fn mirror(&mut self, other: &EntityProperty) {
        self.inner.mirror(other);
    }

    /// Returns the `RemoteEntity` this property is still waiting to resolve, or `None` if already resolved.
    pub fn waiting_remote_entity(&self) -> Option<RemoteEntity> {
        self.inner.waiting_remote_entity()
    }

    /// Writes the resolved local entity value; used when splitting component updates on the receive side.
    // used for writing out ready local entity value when splitting component updates
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
struct HostCreatedRelation {
    global_entity: Option<GlobalEntity>,
    mutator: Option<PropertyMutator>,
    index: u8,
}

impl HostCreatedRelation {
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

        // info!("HostCreatedRelation::write() `global_entity`: {:?}", global_entity);

        let Ok(owned_local_entity) = converter.get_or_reserve_entity(global_entity) else {
            false.ser(writer);
            return;
        };

        // info!("HostCreatedRelation::write() writing `local_entity`: {:?}", owned_local_entity);

        // Must reverse the LocalEntity because the Host<->Remote
        // relationship inverts after this data goes over the wire
        let reversed_local_entity = owned_local_entity.to_reversed();

        true.ser(writer);
        reversed_local_entity.ser(writer);
    }

    pub fn bit_length(&self, converter: &mut dyn LocalEntityAndGlobalEntityConverterMut) -> u32 {
        let mut bit_counter = BitCounter::new(0, 0, u32::MAX);
        self.write(&mut bit_counter, converter);
        bit_counter.bits_needed()
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
        self.global_entity = *other_global_entity;
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
struct RemoteCreatedRelation {
    global_entity: Option<GlobalEntity>,
}

impl RemoteCreatedRelation {
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
        let Ok(owned_entity) = converter.global_entity_to_owned_entity(global_entity) else {
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
        bit_counter.bits_needed()
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
        let Ok(owned_entity) = converter.global_entity_to_owned_entity(global_entity) else {
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
        }
    }

    pub fn set_to_none(&mut self) {
        self.global_entity = None;
        self.mutate();
    }

    pub fn set_global_entity(&mut self, other_global_entity: &Option<GlobalEntity>) {
        self.global_entity = *other_global_entity;
        self.mutate();
    }

    pub fn mirror_waiting(&mut self) {
        self.global_entity = None;
        self.mutate();
    }

    pub fn read_none(mut self) -> Self {
        if self.can_read() {
            self.global_entity = None;
            // Applying a *remote* update is a read-path operation; the mutator
            // call only re-queues the property for onward replication. Guard it
            // separately, exactly as `DelegatedProperty::read` already does --
            // on a client `can_read` and `can_mutate` are complements, so
            // calling `mutate()` unconditionally here panicked on every remote
            // update the host was allowed to read.
            if self.can_mutate() {
                self.mutate();
            }
        }

        self
    }

    pub fn read_some(mut self, global_entity: GlobalEntity) -> Self {
        if self.can_read() {
            self.global_entity = Some(global_entity);
            // See `read_none` above.
            if self.can_mutate() {
                self.mutate();
            }
        }

        self
    }

    pub fn bit_length(&self, converter: &mut dyn LocalEntityAndGlobalEntityConverterMut) -> u32 {
        if !self.can_write() {
            panic!("Must have Authority over Entity before performing this operation.");
        }
        let mut bit_counter = BitCounter::new(0, 0, u32::MAX);
        self.write(&mut bit_counter, converter);
        bit_counter.bits_needed()
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
        let Ok(host_entity) = converter.global_entity_to_owned_entity(global_entity) else {
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
        }
    }

    pub fn set_to_none(&mut self) {
        self.global_entity = None;
    }

    pub fn mirror_waiting(&mut self) {
        self.global_entity = None;
    }

    pub fn set_global_entity(&mut self, other_global_entity: &Option<GlobalEntity>) {
        self.global_entity = *other_global_entity;
    }
}

#[cfg(test)]
mod delegated_auth_tests {
    //! Regression coverage for the delegated-authority invariant family found by
    //! the Cyberlith NPA promotion gate (two roots, one bug shape: an authority
    //! predicate checked at one moment being relied on at another).
    //!
    //! Root 1, covered here: `DelegatedRelation::read_some`/`read_none` apply a
    //! *remote* update. They guard on `can_read()` and then call `mutate()`,
    //! which asserts `can_mutate()`. On a client those two predicates are
    //! complements, so the guard passing *guarantees* the assert fails.

    use super::*;
    use crate::{
        world::delegation::{
            auth_channel::EntityAuthChannel, entity_auth_status::EntityAuthStatus,
        },
        BigMapKey, HostType, PropertyMutate, PropertyMutator,
    };
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[derive(Clone)]
    struct CountingMutator(Arc<AtomicUsize>);

    impl PropertyMutate for CountingMutator {
        fn mutate(&mut self, _property_index: u8) -> bool {
            self.0.fetch_add(1, Ordering::Relaxed);
            true
        }
    }

    fn relation_at(status: EntityAuthStatus) -> (DelegatedRelation, Arc<AtomicUsize>) {
        let (mutator_handle, accessor) = EntityAuthChannel::new_channel(HostType::Client);
        mutator_handle.set_auth_status(status);
        let count = Arc::new(AtomicUsize::new(0));
        let prop_mutator = PropertyMutator::new(CountingMutator(count.clone()));
        (
            DelegatedRelation::new(None, &accessor, &prop_mutator, 0),
            count,
        )
    }

    /// The exact panic the NPA repro hits: a second client receives a remote
    /// update for a delegated entity it does not own (`Available`).
    #[test]
    fn applies_a_remote_update_when_readable_but_not_mutable() {
        for status in [
            EntityAuthStatus::Available,
            EntityAuthStatus::Releasing,
            EntityAuthStatus::Denied,
        ] {
            let (relation, _) = relation_at(status);
            assert!(
                relation.can_read() && !relation.can_mutate(),
                "{status:?} must be the readable-but-not-mutable case this test is about",
            );
            let target = GlobalEntity::from_u64(7);
            let relation = relation.read_some(target);
            assert_eq!(
                relation.global_entity,
                Some(target),
                "a readable remote update must be applied at {status:?}",
            );
        }
    }

    #[test]
    fn applies_a_remote_clear_when_readable_but_not_mutable() {
        for status in [
            EntityAuthStatus::Available,
            EntityAuthStatus::Releasing,
            EntityAuthStatus::Denied,
        ] {
            let (mut relation, _) = relation_at(status);
            relation.global_entity = Some(GlobalEntity::from_u64(7));
            let relation = relation.read_none();
            assert_eq!(
                relation.global_entity, None,
                "a readable remote clear must be applied at {status:?}",
            );
        }
    }

    /// The mutator call is what re-queues the property for onward replication.
    /// It must still fire when the host *can* mutate, or a client that owns the
    /// entity would silently stop propagating remote updates.
    #[test]
    fn still_notifies_the_mutator_when_the_host_can_mutate() {
        let (mutator_handle, accessor) = EntityAuthChannel::new_channel(HostType::Server);
        mutator_handle.set_auth_status(EntityAuthStatus::Granted);
        let count = Arc::new(AtomicUsize::new(0));
        let prop_mutator = PropertyMutator::new(CountingMutator(count.clone()));
        let relation = DelegatedRelation::new(None, &accessor, &prop_mutator, 0);
        assert!(relation.can_read() && relation.can_mutate());

        let relation = relation.read_some(GlobalEntity::from_u64(7));
        assert_eq!(relation.global_entity, Some(GlobalEntity::from_u64(7)));
        assert_eq!(
            count.load(Ordering::Relaxed),
            1,
            "a mutable host must still mark the property dirty",
        );
    }

    fn panic_message_of(body: impl FnOnce()) -> Option<String> {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
        std::panic::set_hook(previous);
        result.err().map(|payload| {
            payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
                .unwrap_or_else(|| "<non-string panic payload>".to_string())
        })
    }

    /// Audit of `DelegatedRelation`'s remaining unguarded `mutate()` calls
    /// (call-site audit, item 2): `set_global_entity`, `set_to_none` and
    /// `mirror_waiting`.
    ///
    /// These are the `EntityProperty` mirror of `DelegatedProperty::mirror`,
    /// and the audit reaches the same conclusion: every internal caller runs
    /// through `EntityRelation::mirror`, which is reached only from
    /// `Replicate::mirror` at the three call sites gated in
    /// `client/src/client.rs:insert_component` (client, `Granted` only) and on
    /// the server (mutable in every status). The predicate cannot drift across
    /// those calls, so the panic is a user-facing contract -- the price of
    /// calling `EntityProperty::set`/`set_to_none` without authority -- rather
    /// than root 1's guaranteed self-inflicted crash on the receive path.
    ///
    /// This test pins the contract on all three entry points at once, so a
    /// caller added without a gate fails in naia's own suite.
    #[test]
    fn the_relation_mutators_refuse_a_client_that_may_not_mutate() {
        type Op = (&'static str, fn(&mut DelegatedRelation));
        let ops: [Op; 3] = [
            ("set_global_entity", |relation| {
                relation.set_global_entity(&Some(GlobalEntity::from_u64(7)))
            }),
            ("set_to_none", |relation| relation.set_to_none()),
            ("mirror_waiting", |relation| relation.mirror_waiting()),
        ];
        for status in [
            EntityAuthStatus::Available,
            EntityAuthStatus::Releasing,
            EntityAuthStatus::Denied,
        ] {
            for (name, op) in ops {
                let (mut relation, count) = relation_at(status);
                assert!(!relation.can_mutate());
                let message = panic_message_of(|| op(&mut relation));
                assert!(
                    message
                        .as_deref()
                        .is_some_and(|m| m.contains("Must request authority to mutate")),
                    "{name} at {status:?} must panic with the authority contract \
                     message, got {message:?}",
                );
                assert_eq!(
                    count.load(Ordering::Relaxed),
                    0,
                    "{name} at {status:?} must not have marked the property dirty",
                );
            }
        }
    }

    #[test]
    fn the_relation_mutators_still_work_wherever_the_client_may_mutate() {
        for status in [EntityAuthStatus::Requested, EntityAuthStatus::Granted] {
            let (mut relation, count) = relation_at(status);
            assert!(relation.can_mutate());
            let target = GlobalEntity::from_u64(7);
            relation.set_global_entity(&Some(target));
            assert_eq!(relation.global_entity, Some(target));
            relation.set_to_none();
            assert_eq!(relation.global_entity, None);
            assert_eq!(
                count.load(Ordering::Relaxed),
                2,
                "both mutators must mark the property dirty at {status:?}",
            );
        }
    }
}

#[cfg(test)]
mod relation_state_machine_tests {
    //! Coverage for the `EntityProperty` relation state machine: construction,
    //! the wire round trip, the nine `read` cases, every lifecycle transition,
    //! and the panic contracts that guard each of them.

    use super::*;
    use crate::{
        world::{
            delegation::{auth_channel::EntityAuthChannel, entity_auth_status::EntityAuthStatus},
            local::local_entity::HostEntity,
        },
        BigMapKey, EntityDoesNotExistError, HostType, PropertyMutate, PropertyMutator,
    };
    use std::{
        collections::{HashMap, HashSet},
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    };

    // -- fixtures ----------------------------------------------------------

    #[derive(Clone)]
    struct CountingMutator(Arc<AtomicUsize>);

    impl PropertyMutate for CountingMutator {
        fn mutate(&mut self, _property_index: u8) -> bool {
            self.0.fetch_add(1, Ordering::Relaxed);
            true
        }
    }

    fn counting_mutator() -> (PropertyMutator, Arc<AtomicUsize>) {
        let count = Arc::new(AtomicUsize::new(0));
        (PropertyMutator::new(CountingMutator(count.clone())), count)
    }

    fn accessor_at(host_type: HostType, status: EntityAuthStatus) -> EntityAuthAccessor {
        let (handle, accessor) = EntityAuthChannel::new_channel(host_type);
        handle.set_auth_status(status);
        accessor
    }

    /// A server accessor: readable, writable and mutable in every status, which
    /// is what the non-authority tests want out of the way.
    fn full_authority() -> EntityAuthAccessor {
        accessor_at(HostType::Server, EntityAuthStatus::Granted)
    }

    /// A converter whose entity space is exactly `known`: the raw `u64` of a
    /// `GlobalEntity` is also its local id, in both the host and remote pools.
    /// Anything outside `known` fails to convert, which is how the tests reach
    /// the not-found branch of every conversion.
    struct MapConverter {
        known: HashSet<u64>,
        redirects: HashMap<OwnedLocalEntity, OwnedLocalEntity>,
    }

    impl MapConverter {
        fn with(known: &[u64]) -> Self {
            Self {
                known: known.iter().copied().collect(),
                redirects: HashMap::new(),
            }
        }
        fn empty() -> Self {
            Self::with(&[])
        }
        fn redirecting(known: &[u64], from: OwnedLocalEntity, to: OwnedLocalEntity) -> Self {
            let mut this = Self::with(known);
            this.redirects.insert(from, to);
            this
        }
        fn check(&self, id: u64) -> Result<GlobalEntity, EntityDoesNotExistError> {
            if self.known.contains(&id) {
                Ok(GlobalEntity::from_u64(id))
            } else {
                Err(EntityDoesNotExistError)
            }
        }
    }

    impl LocalEntityAndGlobalEntityConverter for MapConverter {
        fn global_entity_to_host_entity(
            &self,
            global_entity: &GlobalEntity,
        ) -> Result<HostEntity, EntityDoesNotExistError> {
            self.check(global_entity.to_u64())
                .map(|_| HostEntity::new(global_entity.to_u64() as u32))
        }
        fn global_entity_to_remote_entity(
            &self,
            global_entity: &GlobalEntity,
        ) -> Result<RemoteEntity, EntityDoesNotExistError> {
            self.check(global_entity.to_u64())
                .map(|_| RemoteEntity::new(global_entity.to_u64() as u32))
        }
        fn global_entity_to_owned_entity(
            &self,
            global_entity: &GlobalEntity,
        ) -> Result<OwnedLocalEntity, EntityDoesNotExistError> {
            self.check(global_entity.to_u64())
                .map(|_| OwnedLocalEntity::new_host_dynamic(global_entity.to_u64() as u32))
        }
        fn host_entity_to_global_entity(
            &self,
            host_entity: &HostEntity,
        ) -> Result<GlobalEntity, EntityDoesNotExistError> {
            self.check(host_entity.value() as u64)
        }
        fn static_host_entity_to_global_entity(
            &self,
            host_entity: &HostEntity,
        ) -> Result<GlobalEntity, EntityDoesNotExistError> {
            self.check(host_entity.value() as u64)
        }
        fn remote_entity_to_global_entity(
            &self,
            remote_entity: &RemoteEntity,
        ) -> Result<GlobalEntity, EntityDoesNotExistError> {
            self.check(remote_entity.value() as u64)
        }
        fn apply_entity_redirect(&self, entity: &OwnedLocalEntity) -> OwnedLocalEntity {
            self.redirects.get(entity).copied().unwrap_or(*entity)
        }
    }

    impl LocalEntityAndGlobalEntityConverterMut for MapConverter {
        fn get_or_reserve_entity(
            &mut self,
            global_entity: &GlobalEntity,
        ) -> Result<OwnedLocalEntity, EntityDoesNotExistError> {
            self.global_entity_to_owned_entity(global_entity)
        }
    }

    impl EntityAndGlobalEntityConverter<u64> for MapConverter {
        fn global_entity_to_entity(
            &self,
            global_entity: &GlobalEntity,
        ) -> Result<u64, EntityDoesNotExistError> {
            self.check(global_entity.to_u64()).map(|g| g.to_u64())
        }
        fn entity_to_global_entity(
            &self,
            entity: &u64,
        ) -> Result<GlobalEntity, EntityDoesNotExistError> {
            self.check(*entity)
        }
    }

    fn global(id: u64) -> GlobalEntity {
        GlobalEntity::from_u64(id)
    }

    // -- relation constructors --------------------------------------------

    fn host_created(entity: Option<u64>) -> EntityProperty {
        let mut inner = HostCreatedRelation::new();
        inner.global_entity = entity.map(global);
        EntityProperty {
            inner: EntityRelation::HostCreated(inner),
        }
    }
    fn remote_created(entity: Option<u64>) -> EntityProperty {
        EntityProperty {
            inner: EntityRelation::RemoteCreated(RemoteCreatedRelation::new_with_value(
                entity.map(global),
            )),
        }
    }
    fn remote_public(entity: Option<u64>) -> (EntityProperty, Arc<AtomicUsize>) {
        let (mutator, count) = counting_mutator();
        (
            EntityProperty {
                inner: EntityRelation::RemotePublic(RemotePublicRelation::new(
                    entity.map(global),
                    3,
                    &mutator,
                )),
            },
            count,
        )
    }
    fn delegated(entity: Option<u64>) -> (EntityProperty, Arc<AtomicUsize>) {
        delegated_with(entity, full_authority())
    }
    fn delegated_with(
        entity: Option<u64>,
        accessor: EntityAuthAccessor,
    ) -> (EntityProperty, Arc<AtomicUsize>) {
        let (mutator, count) = counting_mutator();
        (
            EntityProperty {
                inner: EntityRelation::Delegated(DelegatedRelation::new(
                    entity.map(global),
                    &accessor,
                    &mutator,
                    5,
                )),
            },
            count,
        )
    }
    fn local(entity: Option<u64>) -> EntityProperty {
        EntityProperty {
            inner: EntityRelation::Local(LocalRelation::new(entity.map(global))),
        }
    }
    fn waiting(remote_id: u32) -> EntityProperty {
        EntityProperty {
            inner: EntityRelation::RemoteWaiting(RemoteWaitingRelation::new(RemoteEntity::new(
                remote_id,
            ))),
        }
    }
    fn invalid() -> EntityProperty {
        EntityProperty {
            inner: EntityRelation::Invalid,
        }
    }

    fn panic_message_of(body: impl FnOnce()) -> Option<String> {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
        std::panic::set_hook(previous);
        result.err().map(|payload| {
            payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
                .unwrap_or_else(|| "<non-string panic payload>".to_string())
        })
    }

    /// Serializes the `exists`+`OwnedLocalEntity` pair that every read path expects.
    fn wire_bits(local_entity: Option<OwnedLocalEntity>) -> Box<[u8]> {
        let mut writer = BitWriter::new();
        match local_entity {
            Some(entity) => {
                true.ser(&mut writer);
                entity.ser(&mut writer);
            }
            None => false.ser(&mut writer),
        }
        writer.to_bytes()
    }

    /// Reads back the `exists`+`OwnedLocalEntity` pair a property just wrote.
    fn decode(bytes: &[u8]) -> Option<OwnedLocalEntity> {
        let mut reader = BitReader::new(bytes);
        if bool::de(&mut reader).unwrap() {
            Some(OwnedLocalEntity::de(&mut reader).unwrap())
        } else {
            None
        }
    }

    // -- construction, get, set -------------------------------------------

    #[test]
    fn a_new_property_is_host_created_and_empty() {
        for property in [
            EntityProperty::new_for_message(),
            EntityProperty::new_for_component(4),
        ] {
            assert_eq!(property.inner.name(), "HostOwned");
            assert_eq!(property.get_inner(), None);
            assert_eq!(property.get(&MapConverter::with(&[1])), None::<u64>);
        }
    }

    #[test]
    fn a_component_property_remembers_its_mutator_index() {
        let mut property = EntityProperty::new_for_component(9);
        let (mutator, count) = counting_mutator();
        property.set_mutator(&mutator);
        let EntityRelation::HostCreated(inner) = &property.inner else {
            panic!("expected a host relation");
        };
        assert_eq!(inner.index, 9, "the mutate index must survive construction");
        property.set(&MapConverter::with(&[7]), &7u64);
        assert_eq!(count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn setting_and_clearing_a_host_property_notifies_the_mutator() {
        let mut property = EntityProperty::new_for_component(0);
        let (mutator, count) = counting_mutator();
        property.set_mutator(&mutator);
        let converter = MapConverter::with(&[7]);

        property.set(&converter, &7u64);
        assert_eq!(property.get_inner(), Some(global(7)));
        assert_eq!(property.get(&converter), Some(7u64));

        property.set_to_none();
        assert_eq!(property.get_inner(), None);
        assert_eq!(
            count.load(Ordering::Relaxed),
            2,
            "both the set and the clear must mark the property dirty",
        );
    }

    #[test]
    fn a_property_with_no_mutator_can_still_be_set() {
        let mut property = EntityProperty::new_for_message();
        property.set(&MapConverter::with(&[7]), &7u64);
        assert_eq!(property.get_inner(), Some(global(7)));
    }

    #[test]
    fn setting_an_unresolvable_entity_leaves_the_property_alone() {
        let mut property = host_created(Some(7));
        property.set(&MapConverter::empty(), &99u64);
        assert_eq!(
            property.get_inner(),
            Some(global(7)),
            "a failed conversion must not clobber the existing value",
        );
    }

    #[test]
    fn get_reports_none_when_the_global_entity_cannot_be_resolved() {
        assert_eq!(
            host_created(Some(7)).get(&MapConverter::empty()),
            None::<u64>
        );
    }

    #[test]
    fn setting_a_local_or_delegated_property_works_and_a_remote_one_panics() {
        let converter = MapConverter::with(&[7]);

        let mut local_property = local(None);
        local_property.set(&converter, &7u64);
        assert_eq!(local_property.get_inner(), Some(global(7)));
        local_property.set_to_none();
        assert_eq!(local_property.get_inner(), None);

        let (mut delegated_property, count) = delegated(None);
        delegated_property.set(&converter, &7u64);
        assert_eq!(delegated_property.get_inner(), Some(global(7)));
        delegated_property.set_to_none();
        assert_eq!(delegated_property.get_inner(), None);
        assert_eq!(count.load(Ordering::Relaxed), 2);

        for mut property in [
            remote_created(Some(7)),
            waiting(2),
            remote_public(Some(7)).0,
            invalid(),
        ] {
            let name = property.inner.name().to_string();
            for message in [
                panic_message_of(|| property.set(&converter, &7u64)),
                panic_message_of(|| property.set_to_none()),
            ] {
                assert!(
                    message
                        .as_deref()
                        .is_some_and(|m| m.contains("should never be set manually")),
                    "{name} must refuse a manual set, got {message:?}",
                );
            }
        }
    }

    #[test]
    fn setting_an_unresolvable_entity_on_a_local_property_leaves_it_alone() {
        let mut property = local(Some(7));
        property.set(&MapConverter::empty(), &99u64);
        assert_eq!(property.get_inner(), Some(global(7)));
    }

    #[test]
    fn setting_an_unresolvable_entity_on_a_delegated_property_leaves_it_alone() {
        let (mut property, count) = delegated(Some(7));
        property.set(&MapConverter::empty(), &99u64);
        assert_eq!(property.get_inner(), Some(global(7)));
        assert_eq!(
            count.load(Ordering::Relaxed),
            0,
            "a failed conversion must not mark the property dirty",
        );
    }

    #[test]
    fn set_mutator_only_applies_to_host_properties() {
        let (mutator, _) = counting_mutator();
        for mut property in [
            remote_created(None),
            waiting(1),
            remote_public(None).0,
            local(None),
            delegated(None).0,
            invalid(),
        ] {
            let name = property.inner.name().to_string();
            let message = panic_message_of(|| property.set_mutator(&mutator));
            assert!(
                message
                    .as_deref()
                    .is_some_and(|m| m.contains("cannot call set_mutator()")),
                "{name} must refuse set_mutator, got {message:?}",
            );
        }
    }

    // -- write / bit_length ------------------------------------------------

    #[test]
    fn a_writable_property_reverses_the_entity_onto_the_wire() {
        let mut converter = MapConverter::with(&[7]);
        let writable: Vec<EntityProperty> = vec![
            host_created(Some(7)),
            remote_public(Some(7)).0,
            delegated(Some(7)).0,
        ];
        for property in writable {
            let name = property.inner.name().to_string();
            let mut writer = BitWriter::new();
            property.write(&mut writer, &mut converter);
            assert_eq!(
                decode(&writer.to_bytes()),
                Some(OwnedLocalEntity::new_remote_dynamic(7)),
                "{name} must reverse its host entity for the far side",
            );
        }
    }

    #[test]
    fn an_empty_or_unresolvable_property_writes_a_bare_absence_flag() {
        for (label, property, mut converter) in [
            ("empty host", host_created(None), MapConverter::with(&[7])),
            ("unknown host", host_created(Some(7)), MapConverter::empty()),
            (
                "empty public",
                remote_public(None).0,
                MapConverter::with(&[7]),
            ),
            (
                "unknown public",
                remote_public(Some(7)).0,
                MapConverter::empty(),
            ),
            (
                "empty delegated",
                delegated(None).0,
                MapConverter::with(&[7]),
            ),
            (
                "unknown delegated",
                delegated(Some(7)).0,
                MapConverter::empty(),
            ),
        ] {
            let mut writer = BitWriter::new();
            property.write(&mut writer, &mut converter);
            assert_eq!(
                decode(&writer.to_bytes()),
                None,
                "{label} must write absent"
            );
        }
    }

    #[test]
    fn bit_length_matches_the_bits_actually_written() {
        let mut converter = MapConverter::with(&[7]);
        for property in [
            host_created(Some(7)),
            host_created(None),
            remote_public(Some(7)).0,
            delegated(Some(7)).0,
        ] {
            let mut writer = BitWriter::new();
            property.write(&mut writer, &mut converter);
            let written = writer.bits_written();
            assert_eq!(
                property.bit_length(&mut converter),
                written,
                "{} must count exactly what it writes",
                property.inner.name(),
            );
        }
    }

    #[test]
    fn an_unwritable_property_refuses_to_write_or_be_measured() {
        let mut converter = MapConverter::with(&[7]);
        for property in [
            remote_created(Some(7)),
            waiting(1),
            local(Some(7)),
            invalid(),
        ] {
            let name = property.inner.name().to_string();
            let write_message = panic_message_of(|| {
                let mut writer = BitWriter::new();
                property.write(&mut writer, &mut MapConverter::with(&[7]));
            });
            assert!(
                write_message
                    .as_deref()
                    .is_some_and(|m| m.contains("should never be written")),
                "{name} must refuse write, got {write_message:?}",
            );
            let length_message = panic_message_of(|| {
                let _ = property.bit_length(&mut converter);
            });
            assert!(
                length_message
                    .as_deref()
                    .is_some_and(|m| m.contains("should never be written")),
                "{name} must refuse bit_length, got {length_message:?}",
            );
        }
    }

    #[test]
    fn a_delegated_property_without_authority_refuses_to_write() {
        let (property, _) = delegated_with(
            Some(7),
            accessor_at(HostType::Client, EntityAuthStatus::Available),
        );
        let mut converter = MapConverter::with(&[7]);
        let write_message = panic_message_of(|| {
            let mut writer = BitWriter::new();
            property.write(&mut writer, &mut MapConverter::with(&[7]));
        });
        let length_message = panic_message_of(|| {
            let _ = property.bit_length(&mut converter);
        });
        for message in [write_message, length_message] {
            assert!(
                message
                    .as_deref()
                    .is_some_and(|m| m.contains("Must have Authority over Entity")),
                "an unauthorized delegated write must be refused, got {message:?}",
            );
        }
    }

    // -- write_local_entity ------------------------------------------------

    #[test]
    fn write_local_entity_writes_the_unreversed_entity() {
        let converter = MapConverter::with(&[7]);
        for property in [
            remote_created(Some(7)),
            remote_public(Some(7)).0,
            delegated(Some(7)).0,
        ] {
            let name = property.inner.name().to_string();
            let mut writer = BitWriter::new();
            property.write_local_entity(&converter, &mut writer);
            assert_eq!(
                decode(&writer.to_bytes()),
                Some(OwnedLocalEntity::new_host_dynamic(7)),
                "{name} must write the local entity as-is, without reversing it",
            );
        }
    }

    #[test]
    fn write_local_entity_writes_absent_when_there_is_nothing_to_resolve() {
        for (label, property, converter) in [
            (
                "empty remote",
                remote_created(None),
                MapConverter::with(&[7]),
            ),
            (
                "unknown remote",
                remote_created(Some(7)),
                MapConverter::empty(),
            ),
            (
                "empty public",
                remote_public(None).0,
                MapConverter::with(&[7]),
            ),
            (
                "unknown public",
                remote_public(Some(7)).0,
                MapConverter::empty(),
            ),
            (
                "empty delegated",
                delegated(None).0,
                MapConverter::with(&[7]),
            ),
            (
                "unknown delegated",
                delegated(Some(7)).0,
                MapConverter::empty(),
            ),
        ] {
            let mut writer = BitWriter::new();
            property.write_local_entity(&converter, &mut writer);
            assert_eq!(
                decode(&writer.to_bytes()),
                None,
                "{label} must write absent"
            );
        }
    }

    #[test]
    fn write_local_entity_refuses_the_relations_that_have_no_local_view() {
        let converter = MapConverter::with(&[7]);
        for property in [host_created(Some(7)), waiting(1), local(Some(7)), invalid()] {
            let name = property.inner.name().to_string();
            let message = panic_message_of(|| {
                let mut writer = BitWriter::new();
                property.write_local_entity(&MapConverter::with(&[7]), &mut writer);
            });
            assert!(
                message
                    .as_deref()
                    .is_some_and(|m| m.contains("can't use this method")),
                "{name} must refuse write_local_entity, got {message:?}",
            );
        }
        drop(converter);
    }

    // -- new_read / read_write ---------------------------------------------

    #[test]
    fn new_read_resolves_a_known_entity() {
        let bytes = wire_bits(Some(OwnedLocalEntity::new_remote_dynamic(7)));
        let mut reader = BitReader::new(&bytes);
        let property = EntityProperty::new_read(&mut reader, &MapConverter::with(&[7])).unwrap();
        assert_eq!(property.inner.name(), "RemoteOwned");
        assert_eq!(property.get_inner(), Some(global(7)));
    }

    #[test]
    fn new_read_of_an_absent_entity_yields_an_empty_remote_property() {
        let bytes = wire_bits(None);
        let mut reader = BitReader::new(&bytes);
        let property = EntityProperty::new_read(&mut reader, &MapConverter::empty()).unwrap();
        assert_eq!(property.inner.name(), "RemoteOwned");
        assert_eq!(property.get_inner(), None);
    }

    #[test]
    fn new_read_of_an_out_of_scope_remote_entity_waits_for_it() {
        let bytes = wire_bits(Some(OwnedLocalEntity::new_remote_dynamic(7)));
        let mut reader = BitReader::new(&bytes);
        let property = EntityProperty::new_read(&mut reader, &MapConverter::empty()).unwrap();
        assert_eq!(property.inner.name(), "RemoteWaiting");
        assert_eq!(property.waiting_remote_entity(), Some(RemoteEntity::new(7)));
    }

    #[test]
    fn new_read_of_an_unresolvable_host_entity_is_invalid() {
        let bytes = wire_bits(Some(OwnedLocalEntity::new_host_dynamic(7)));
        let mut reader = BitReader::new(&bytes);
        let property = EntityProperty::new_read(&mut reader, &MapConverter::empty()).unwrap();
        assert_eq!(
            property.inner.name(),
            "Invalid",
            "a host reference that cannot be resolved has nothing to wait for",
        );
    }

    #[test]
    fn new_read_follows_an_entity_redirect() {
        let converter = MapConverter::redirecting(
            &[7],
            OwnedLocalEntity::new_remote_dynamic(3),
            OwnedLocalEntity::new_remote_dynamic(7),
        );
        let bytes = wire_bits(Some(OwnedLocalEntity::new_remote_dynamic(3)));
        let mut reader = BitReader::new(&bytes);
        let property = EntityProperty::new_read(&mut reader, &converter).unwrap();
        assert_eq!(
            property.get_inner(),
            Some(global(7)),
            "a migrated entity must resolve through its redirect",
        );
    }

    #[test]
    fn read_write_copies_the_field_verbatim() {
        for source in [None, Some(OwnedLocalEntity::new_remote_static(9))] {
            let bytes = wire_bits(source);
            let mut reader = BitReader::new(&bytes);
            let mut writer = BitWriter::new();
            EntityProperty::read_write(&mut reader, &mut writer).unwrap();
            assert_eq!(decode(&writer.to_bytes()), source);
        }
    }

    // -- read: the nine relation cases -------------------------------------

    fn read_into(property: &mut EntityProperty, wire: Option<OwnedLocalEntity>, known: &[u64]) {
        let bytes = wire_bits(wire);
        let mut reader = BitReader::new(&bytes);
        property
            .read(&mut reader, &MapConverter::with(known))
            .unwrap();
    }

    #[test]
    fn reading_into_a_plain_remote_property_covers_its_three_cases() {
        let known = OwnedLocalEntity::new_remote_dynamic(7);
        let unknown = OwnedLocalEntity::new_remote_dynamic(8);

        let mut property = remote_created(Some(7));
        read_into(&mut property, None, &[7]);
        assert_eq!(property.inner.name(), "RemoteOwned");
        assert_eq!(property.get_inner(), None);

        let mut property = remote_created(None);
        read_into(&mut property, Some(known), &[7]);
        assert_eq!(property.inner.name(), "RemoteOwned");
        assert_eq!(property.get_inner(), Some(global(7)));

        let mut property = remote_created(Some(7));
        read_into(&mut property, Some(unknown), &[7]);
        assert_eq!(property.inner.name(), "RemoteWaiting");
        assert_eq!(property.waiting_remote_entity(), Some(RemoteEntity::new(8)));
    }

    #[test]
    fn reading_into_a_public_property_keeps_it_public_and_covers_its_three_cases() {
        let known = OwnedLocalEntity::new_remote_dynamic(7);
        let unknown = OwnedLocalEntity::new_remote_dynamic(8);

        let (mut property, _) = remote_public(Some(7));
        read_into(&mut property, None, &[7]);
        assert_eq!(property.inner.name(), "RemotePublic");
        assert_eq!(property.get_inner(), None);

        let (mut property, _) = remote_public(None);
        read_into(&mut property, Some(known), &[7]);
        assert_eq!(property.inner.name(), "RemotePublic");
        assert_eq!(property.get_inner(), Some(global(7)));

        let (mut property, _) = remote_public(Some(7));
        read_into(&mut property, Some(unknown), &[7]);
        assert_eq!(
            property.inner.name(),
            "RemoteWaiting",
            "an out-of-scope entity parks the property, but it must remember it will publish",
        );
        let EntityRelation::RemoteWaiting(inner) = &property.inner else {
            panic!("expected a waiting relation");
        };
        assert_eq!(
            inner.will_publish.as_ref().map(|(index, _)| *index),
            Some(3)
        );
        assert!(inner.will_delegate.is_none());
    }

    #[test]
    fn reading_into_a_delegated_property_keeps_it_delegated_and_covers_its_three_cases() {
        let known = OwnedLocalEntity::new_remote_dynamic(7);
        let unknown = OwnedLocalEntity::new_remote_dynamic(8);

        let (mut property, _) = delegated(Some(7));
        read_into(&mut property, None, &[7]);
        assert_eq!(property.inner.name(), "Delegated");
        assert_eq!(property.get_inner(), None);

        let (mut property, _) = delegated(None);
        read_into(&mut property, Some(known), &[7]);
        assert_eq!(property.inner.name(), "Delegated");
        assert_eq!(property.get_inner(), Some(global(7)));

        let (mut property, _) = delegated(Some(7));
        read_into(&mut property, Some(unknown), &[7]);
        assert_eq!(property.inner.name(), "RemoteWaiting");
        let EntityRelation::RemoteWaiting(inner) = &property.inner else {
            panic!("expected a waiting relation");
        };
        assert_eq!(
            inner.will_publish.as_ref().map(|(index, _)| *index),
            Some(5),
            "the delegated property's mutate index must survive the park",
        );
        assert!(
            inner.will_delegate.is_some(),
            "a parked delegated property must remember it will delegate",
        );
    }

    #[test]
    fn a_delegated_read_that_may_not_be_applied_is_dropped() {
        let (mut property, count) = delegated_with(
            Some(7),
            accessor_at(HostType::Client, EntityAuthStatus::Granted),
        );
        assert_eq!(property.get_inner(), Some(global(7)));
        read_into(&mut property, None, &[7]);
        assert_eq!(
            property.get_inner(),
            Some(global(7)),
            "a host that owns the entity must ignore the remote's view of it",
        );
        assert_eq!(count.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn reading_an_unresolvable_host_entity_has_nothing_to_wait_on() {
        let mut property = remote_created(None);
        let message = panic_message_of(|| {
            read_into(
                &mut property,
                Some(OwnedLocalEntity::new_host_dynamic(8)),
                &[7],
            )
        });
        assert!(
            message
                .as_deref()
                .is_some_and(|m| m.contains("Expected RemoteEntity")),
            "only remote references can be parked; got {message:?}",
        );
    }

    // -- waiting_complete --------------------------------------------------

    #[test]
    fn completing_a_plain_waiting_property_yields_a_remote_property() {
        let mut property = waiting(7);
        property.waiting_complete(&MapConverter::with(&[7]));
        assert_eq!(property.inner.name(), "RemoteOwned");
        assert_eq!(property.get_inner(), Some(global(7)));
        assert_eq!(property.waiting_remote_entity(), None);
    }

    #[test]
    fn completing_a_waiting_property_that_will_publish_yields_a_public_property() {
        let (mutator, _) = counting_mutator();
        let mut property = waiting(7);
        property.remote_publish(3, &mutator);
        property.waiting_complete(&MapConverter::with(&[7]));
        assert_eq!(property.inner.name(), "RemotePublic");
        assert_eq!(property.get_inner(), Some(global(7)));
        let EntityRelation::RemotePublic(inner) = &property.inner else {
            panic!("expected a public relation");
        };
        assert_eq!(inner.index, 3);
    }

    #[test]
    fn completing_a_waiting_property_that_will_delegate_yields_a_delegated_property() {
        let (mutator, _) = counting_mutator();
        let mut property = waiting(7);
        property.remote_publish(3, &mutator);
        property.enable_delegation(&full_authority(), Some((3, &mutator)));
        property.waiting_complete(&MapConverter::with(&[7]));
        assert_eq!(property.inner.name(), "Delegated");
        assert_eq!(property.get_inner(), Some(global(7)));
        let EntityRelation::Delegated(inner) = &property.inner else {
            panic!("expected a delegated relation");
        };
        assert_eq!(inner.index, 3);
    }

    #[test]
    fn completing_a_waiting_property_follows_an_entity_redirect() {
        let converter = MapConverter::redirecting(
            &[7],
            OwnedLocalEntity::new_remote_dynamic(3),
            OwnedLocalEntity::new_remote_dynamic(7),
        );
        let mut property = waiting(3);
        property.waiting_complete(&converter);
        assert_eq!(property.get_inner(), Some(global(7)));
    }

    #[test]
    fn completing_an_already_resolved_property_changes_nothing() {
        let (public_property, _) = remote_public(Some(7));
        let (delegated_property, _) = delegated(Some(7));
        for mut property in [remote_created(Some(7)), public_property, delegated_property] {
            let name = property.inner.name().to_string();
            property.waiting_complete(&MapConverter::empty());
            assert_eq!(property.inner.name(), name, "{name} must be left alone");
            assert_eq!(property.get_inner(), Some(global(7)));
        }
    }

    #[test]
    fn completing_a_waiting_property_whose_entity_never_arrived_panics() {
        let mut property = waiting(7);
        let message = panic_message_of(|| property.waiting_complete(&MapConverter::empty()));
        assert!(
            message
                .as_deref()
                .is_some_and(|m| m.contains("Error completing waiting EntityProperty")),
            "got {message:?}",
        );
    }

    #[test]
    fn completing_a_property_that_never_waits_panics() {
        for mut property in [host_created(Some(7)), local(Some(7)), invalid()] {
            let name = property.inner.name().to_string();
            let message = panic_message_of(|| property.waiting_complete(&MapConverter::with(&[7])));
            assert!(
                message
                    .as_deref()
                    .is_some_and(|m| m.contains("Can't complete EntityProperty of type")),
                "{name} must refuse waiting_complete, got {message:?}",
            );
        }
    }

    // -- publish / unpublish -----------------------------------------------

    #[test]
    fn publishing_and_unpublishing_a_remote_property_preserves_its_entity() {
        let (mutator, _) = counting_mutator();
        let mut property = remote_created(Some(7));
        property.remote_publish(3, &mutator);
        assert_eq!(property.inner.name(), "RemotePublic");
        assert_eq!(property.get_inner(), Some(global(7)));

        property.remote_unpublish();
        assert_eq!(property.inner.name(), "RemoteOwned");
        assert_eq!(property.get_inner(), Some(global(7)));
    }

    #[test]
    fn publishing_and_unpublishing_a_waiting_property_only_records_the_intent() {
        let (mutator, _) = counting_mutator();
        let mut property = waiting(7);
        property.remote_publish(3, &mutator);
        assert_eq!(property.inner.name(), "RemoteWaiting");
        let EntityRelation::RemoteWaiting(inner) = &property.inner else {
            panic!("expected a waiting relation");
        };
        assert!(inner.will_publish.is_some());

        property.remote_unpublish();
        let EntityRelation::RemoteWaiting(inner) = &property.inner else {
            panic!("expected a waiting relation");
        };
        assert!(
            inner.will_publish.is_none(),
            "unpublishing must clear the recorded intent",
        );
    }

    #[test]
    fn only_a_remote_property_may_be_published() {
        let (mutator, _) = counting_mutator();
        for mut property in [
            host_created(Some(7)),
            remote_public(Some(7)).0,
            local(Some(7)),
            delegated(Some(7)).0,
            invalid(),
        ] {
            let name = property.inner.name().to_string();
            let message = panic_message_of(|| property.remote_publish(3, &mutator));
            assert!(
                message
                    .as_deref()
                    .is_some_and(|m| m.contains("should never be made public twice")),
                "{name} must refuse remote_publish, got {message:?}",
            );
        }
    }

    #[test]
    fn only_a_public_property_may_be_unpublished() {
        for mut property in [
            host_created(Some(7)),
            remote_created(Some(7)),
            local(Some(7)),
            delegated(Some(7)).0,
            invalid(),
        ] {
            let name = property.inner.name().to_string();
            let message = panic_message_of(|| property.remote_unpublish());
            assert!(
                message
                    .as_deref()
                    .is_some_and(|m| m.contains("should never be unpublished")),
                "{name} must refuse remote_unpublish, got {message:?}",
            );
        }
    }

    // -- delegation --------------------------------------------------------

    #[test]
    fn a_host_property_delegates_using_its_own_mutator() {
        let mut property = EntityProperty::new_for_component(6);
        let (mutator, count) = counting_mutator();
        property.set_mutator(&mutator);
        property.set(&MapConverter::with(&[7]), &7u64);

        property.enable_delegation(&full_authority(), None);
        assert_eq!(property.inner.name(), "Delegated");
        assert_eq!(property.get_inner(), Some(global(7)));
        let EntityRelation::Delegated(inner) = &property.inner else {
            panic!("expected a delegated relation");
        };
        assert_eq!(
            inner.index, 6,
            "the host relation's own mutate index must carry over",
        );
        let before = count.load(Ordering::Relaxed);
        property.set_to_none();
        assert_eq!(
            count.load(Ordering::Relaxed),
            before + 1,
            "the delegated relation must keep notifying the original mutator",
        );
    }

    #[test]
    fn a_public_property_delegates_using_its_own_mutator() {
        let (mut property, _) = remote_public(Some(7));
        property.enable_delegation(&full_authority(), None);
        assert_eq!(property.inner.name(), "Delegated");
        assert_eq!(property.get_inner(), Some(global(7)));
        let EntityRelation::Delegated(inner) = &property.inner else {
            panic!("expected a delegated relation");
        };
        assert_eq!(inner.index, 3);
    }

    #[test]
    fn a_remote_property_delegates_only_with_an_explicit_mutator() {
        let (mutator, _) = counting_mutator();
        let mut property = remote_created(Some(7));
        property.enable_delegation(&full_authority(), Some((2, &mutator)));
        assert_eq!(property.inner.name(), "Delegated");
        assert_eq!(property.get_inner(), Some(global(7)));
        let EntityRelation::Delegated(inner) = &property.inner else {
            panic!("expected a delegated relation");
        };
        assert_eq!(inner.index, 2);

        let mut property = remote_created(Some(7));
        let message = panic_message_of(|| property.enable_delegation(&full_authority(), None));
        assert!(
            message
                .as_deref()
                .is_some_and(|m| m.contains("should never enable delegation")),
            "a remote property has no mutator of its own; got {message:?}",
        );
    }

    #[test]
    fn a_waiting_property_only_records_that_it_will_delegate() {
        let (mutator, _) = counting_mutator();
        let mut property = waiting(7);
        property.enable_delegation(&full_authority(), Some((2, &mutator)));
        assert_eq!(property.inner.name(), "RemoteWaiting");
        let EntityRelation::RemoteWaiting(inner) = &property.inner else {
            panic!("expected a waiting relation");
        };
        assert!(inner.will_delegate.is_some());

        property.disable_delegation();
        let EntityRelation::RemoteWaiting(inner) = &property.inner else {
            panic!("expected a waiting relation");
        };
        assert!(
            inner.will_delegate.is_none(),
            "disabling delegation must clear the recorded intent",
        );
    }

    #[test]
    fn disabling_delegation_returns_the_property_to_host_ownership() {
        let (mut property, count) = delegated(Some(7));
        property.disable_delegation();
        assert_eq!(property.inner.name(), "HostOwned");
        assert_eq!(property.get_inner(), Some(global(7)));
        let EntityRelation::HostCreated(inner) = &property.inner else {
            panic!("expected a host relation");
        };
        assert_eq!(inner.index, 5, "the mutate index must carry over");
        property.set_to_none();
        assert_eq!(
            count.load(Ordering::Relaxed),
            1,
            "the restored host relation must keep the delegated relation's mutator",
        );
    }

    #[test]
    fn delegation_cannot_be_enabled_or_disabled_on_the_wrong_relation() {
        let (mutator, _) = counting_mutator();
        for mut property in [
            local(Some(7)),
            remote_public(Some(7)).0,
            host_created(Some(7)),
            delegated(Some(7)).0,
            invalid(),
        ] {
            let name = property.inner.name().to_string();
            let message = panic_message_of(|| {
                property.enable_delegation(&full_authority(), Some((2, &mutator)))
            });
            assert!(
                message
                    .as_deref()
                    .is_some_and(|m| m.contains("should never enable delegation")),
                "{name} must refuse enable_delegation with a mutator, got {message:?}",
            );
        }
        for mut property in [
            local(Some(7)),
            remote_created(Some(7)),
            waiting(7),
            delegated(Some(7)).0,
            invalid(),
        ] {
            let name = property.inner.name().to_string();
            let message = panic_message_of(|| property.enable_delegation(&full_authority(), None));
            assert!(
                message
                    .as_deref()
                    .is_some_and(|m| m.contains("should never enable delegation")),
                "{name} must refuse enable_delegation without a mutator, got {message:?}",
            );
        }
        for mut property in [
            host_created(Some(7)),
            remote_created(Some(7)),
            remote_public(Some(7)).0,
            local(Some(7)),
            invalid(),
        ] {
            let name = property.inner.name().to_string();
            let message = panic_message_of(|| property.disable_delegation());
            assert!(
                message
                    .as_deref()
                    .is_some_and(|m| m.contains("should never disable delegation")),
                "{name} must refuse disable_delegation, got {message:?}",
            );
        }
    }

    // -- localize ----------------------------------------------------------

    #[test]
    fn localizing_keeps_the_entity_and_drops_the_mutator() {
        let (delegated_property, count) = delegated(Some(7));
        for mut property in [host_created(Some(7)), delegated_property] {
            property.localize();
            assert_eq!(property.inner.name(), "Local");
            assert_eq!(property.get_inner(), Some(global(7)));
            property.set_to_none();
            assert_eq!(property.get_inner(), None);
        }
        assert_eq!(
            count.load(Ordering::Relaxed),
            0,
            "a localized property no longer replicates, so it must not mark itself dirty",
        );
    }

    #[test]
    fn only_a_host_or_delegated_property_may_be_localized() {
        for mut property in [
            remote_created(Some(7)),
            remote_public(Some(7)).0,
            waiting(7),
            local(Some(7)),
            invalid(),
        ] {
            let name = property.inner.name().to_string();
            let message = panic_message_of(|| property.localize());
            assert!(
                message
                    .as_deref()
                    .is_some_and(|m| m.contains("should never be made local")),
                "{name} must refuse localize, got {message:?}",
            );
        }
    }

    // -- mirror ------------------------------------------------------------

    #[test]
    fn mirroring_copies_the_entity_into_every_settable_relation() {
        let sources: [(&str, fn() -> EntityProperty); 5] = [
            ("host", || host_created(Some(7))),
            ("remote", || remote_created(Some(7))),
            ("public", || remote_public(Some(7)).0),
            ("local", || local(Some(7))),
            ("delegated", || delegated(Some(7)).0),
        ];
        let targets: [(&str, fn() -> EntityProperty); 3] = [
            ("host", || host_created(None)),
            ("local", || local(None)),
            ("delegated", || delegated(None).0),
        ];
        for (source_name, build_source) in sources {
            for (target_name, build_target) in targets {
                let mut target = build_target();
                target.mirror(&build_source());
                assert_eq!(
                    target.get_inner(),
                    Some(global(7)),
                    "{source_name} -> {target_name} must copy the entity",
                );
            }
        }
    }

    #[test]
    fn mirroring_a_waiting_property_clears_the_target() {
        let targets: [(&str, fn() -> EntityProperty); 3] = [
            ("host", || host_created(Some(7))),
            ("local", || local(Some(7))),
            ("delegated", || delegated(Some(7)).0),
        ];
        for (name, build_target) in targets {
            let mut target = build_target();
            target.mirror(&waiting(9));
            assert_eq!(
                target.get_inner(),
                None,
                "{name} must clear itself rather than copy an unresolved entity",
            );
        }
    }

    #[test]
    fn mirroring_an_invalid_property_panics() {
        let targets: [(&str, fn() -> EntityProperty); 3] = [
            ("host", || host_created(Some(7))),
            ("local", || local(Some(7))),
            ("delegated", || delegated(Some(7)).0),
        ];
        for (name, build_target) in targets {
            let mut target = build_target();
            let message = panic_message_of(|| target.mirror(&invalid()));
            assert!(
                message
                    .as_deref()
                    .is_some_and(|m| m.contains("Invalid EntityProperty should never be mirrored")),
                "{name} must refuse to mirror an invalid property, got {message:?}",
            );
        }
    }

    #[test]
    fn mirroring_into_a_relation_that_cannot_be_set_panics() {
        for mut target in [
            remote_created(None),
            waiting(1),
            remote_public(None).0,
            invalid(),
        ] {
            let name = target.inner.name().to_string();
            let message = panic_message_of(|| target.mirror(&host_created(Some(7))));
            assert!(
                message
                    .as_deref()
                    .is_some_and(|m| m.contains("should never be set manually")),
                "{name} must refuse to be mirrored into, got {message:?}",
            );
        }
    }

    // -- waiting_remote_entity ---------------------------------------------

    #[test]
    fn only_a_waiting_property_reports_a_pending_remote_entity() {
        assert_eq!(
            waiting(9).waiting_remote_entity(),
            Some(RemoteEntity::new(9))
        );
        for property in [
            host_created(Some(7)),
            remote_created(Some(7)),
            remote_public(Some(7)).0,
            local(Some(7)),
            delegated(Some(7)).0,
            invalid(),
        ] {
            let name = property.inner.name().to_string();
            assert_eq!(
                property.waiting_remote_entity(),
                None,
                "{name} is not waiting on anything",
            );
        }
    }

    #[test]
    fn a_waiting_or_invalid_property_holds_no_global_entity() {
        assert_eq!(waiting(9).get_inner(), None);
        assert_eq!(invalid().get_inner(), None);
    }
}
