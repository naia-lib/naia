use std::{any::Any, collections::HashSet};

use naia_serde::{BitReader, BitWrite, SerdeErr};

use crate::world::update::component_update::PendingComponentUpdate;
use crate::world::update::diff_mask::DiffMask;
use crate::{
    named::Named,
    world::{
        component::{
            component_kinds::{ComponentKind, ComponentKinds},
            property_mutate::PropertyMutator,
            replica_ref::{ReplicaDynMut, ReplicaDynRef},
        },
        delegation::auth_channel::EntityAuthAccessor,
        entity::entity_converters::LocalEntityAndGlobalEntityConverter,
    },
    ComponentFieldUpdate, LocalEntityAndGlobalEntityConverterMut, RemoteEntity,
};

/// Result of splitting a component update into a waiting set (unresolved entity refs) and a ready payload.
pub type SplitUpdateResult = Result<
    (
        Option<Vec<(RemoteEntity, ComponentFieldUpdate)>>,
        Option<PendingComponentUpdate>,
    ),
    SerdeErr,
>;

/// Factory trait for deserializing a concrete `Replicate` component or its partial updates from raw bits.
pub trait ReplicateBuilder: Send + Sync + Named {
    /// Returns true if the component type is marked `#[replicate(immutable)]`.
    fn is_immutable(&self) -> bool {
        false
    }
    /// Create new Component from incoming bit stream
    fn read(
        &self,
        reader: &mut BitReader,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) -> Result<Box<dyn Replicate>, SerdeErr>;
    /// Create new Component Update from incoming bit stream
    fn read_create_update(
        &self,
        reader: &mut BitReader,
    ) -> Result<PendingComponentUpdate, SerdeErr>;
    /// Split a Component update into Waiting and Ready updates
    fn split_update(
        &self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        update: PendingComponentUpdate,
    ) -> SplitUpdateResult;

    /// Returns a heap-allocated clone of this builder.
    fn box_clone(&self) -> Box<dyn ReplicateBuilder>;
}

/// A struct that implements Replicate is a Component, or otherwise,
/// a container of Properties that can be scoped, tracked, and synced, with a
/// remote host
pub trait Replicate: Sync + Send + 'static + Named + Any {
    /// Returns true if this component type never sends mutation updates.
    /// Immutable components are written once on spawn and never diff-tracked.
    /// Override in the derive macro by adding `#[replicate(immutable)]`.
    fn is_immutable(&self) -> bool {
        false
    }
    /// True if this component contains one or more `EntityProperty` fields,
    /// meaning its serialized bytes differ per connection and cannot be cached
    /// in a shared `CachedComponentUpdate`. Default: false.
    /// The derive macro overrides to `true` for any component with ≥1 EntityProperty field.
    fn has_entity_properties() -> bool
    where
        Self: Sized,
    {
        false
    }
    /// Upper bound on this component's serialized bit length (all fields dirty).
    /// Returns `u32::MAX` if not precisely known (sentinel — skips the registration
    /// assertion against `CACHED_UPDATE_BITS`).
    /// The derive macro may override with a precise sum via `ConstBitLength` impls.
    fn max_bit_length() -> u32
    where
        Self: Sized,
    {
        u32::MAX
    }
    /// Gets the ComponentKind of this type
    fn kind(&self) -> ComponentKind;
    /// Returns this component's canonical domain descriptor: the domain tag
    /// plus a STRUCT node with one labeled entry per wired property in
    /// declaration order (component facts — immutability, property labels,
    /// mask, entity class, authority — live in the structural registry
    /// entry, not in the descriptor).
    ///
    /// This method is REQUIRED with no default, and deliberately takes no
    /// `Self: WireSchema` bound: component types are not required to derive
    /// `Serde`, and the derive emits this override inline. A descriptor that
    /// silently defaulted would let a registered component travel under a
    /// fingerprint that describes nothing about it, so hand-written
    /// `Replicate` impls must write their own (see the derive's
    /// `get_wire_schema_method` for the exact grammar). This method is never
    /// part of the codec; it only feeds the structural registry and
    /// fingerprint v2.
    ///
    /// The `where Self: Sized` keeps `dyn Replicate` object-safe.
    fn wire_schema() -> Vec<u8>
    where
        Self: Sized;
    /// Ordered labels of this component's *wired* (replicated) properties, in
    /// diff-mask index order. Non-replicated fields never reach the wire and
    /// are excluded — exactly the properties the descriptor's STRUCT node
    /// lists, in the same order.
    ///
    /// REQUIRED with no default: an empty default would desynchronize the
    /// registry entry from the descriptor for hand-written impls. The derive
    /// emits the real list. This method is never part of the codec; it only
    /// feeds the structural registry and fingerprint v2.
    fn replicated_property_labels() -> Vec<&'static str>
    where
        Self: Sized;
    /// Real diff-mask bit index per wired property, parallel to
    /// [`replicated_property_labels`](Self::replicated_property_labels).
    /// These are the sparse declaration-order discriminants (the property
    /// enum's explicit `= index` values), NOT a dense 0..N renumbering:
    /// interleaved non-replicated fields consume positions. REQUIRED with no
    /// default, for the same reason as the labels. Never part of the codec.
    fn property_mask_indices() -> Vec<u8>
    where
        Self: Sized;
    /// Diff-mask size in bytes: `((field_count - 1) / 8) + 1` over ALL
    /// declared fields (replicated or not), `0` for a fieldless component.
    /// This is the static twin of the instance
    /// [`diff_mask_size`](Self::diff_mask_size); it cannot be derived from
    /// the mask indices alone, because trailing non-replicated fields widen
    /// the mask past the highest wired bit. REQUIRED with no default. Never
    /// part of the codec.
    fn mask_size_bytes() -> u8
    where
        Self: Sized;
    /// Labels of this component's `EntityProperty` fields, in declaration
    /// order: the component's entity-relation profile. Entity-typed fields
    /// encode per-connection (the receiver resolves them through its entity
    /// converter), so which labeled properties are entity relations — not
    /// just the [`has_entity_properties`](Self::has_entity_properties) flag
    /// — is a wire-compatibility fact. REQUIRED with no default. Never part
    /// of the codec.
    fn entity_property_labels() -> Vec<&'static str>
    where
        Self: Sized;
    /// Returns a shared `Any` reference for downcasting.
    fn to_any(&self) -> &dyn Any;
    /// Returns a mutable `Any` reference for downcasting.
    fn to_any_mut(&mut self) -> &mut dyn Any;
    /// Converts this boxed component into a `Box<dyn Any>` for downcasting.
    fn to_boxed_any(self: Box<Self>) -> Box<dyn Any>;
    /// Returns a heap-allocated clone of this component as a trait object.
    fn copy_to_box(&self) -> Box<dyn Replicate>;
    /// Creates the `ReplicateBuilder` used to deserialize instances of this type.
    fn create_builder() -> Box<dyn ReplicateBuilder>
    where
        Self: Sized;
    /// Gets the number of bytes of the Component's DiffMask
    fn diff_mask_size(&self) -> u8;
    /// Get an immutable reference to the inner Component as a Replicate trait object
    fn dyn_ref(&self) -> ReplicaDynRef<'_>;
    /// Get an mutable reference to the inner Component as a Replicate trait object
    fn dyn_mut(&mut self) -> ReplicaDynMut<'_>;
    /// Sets the current Component to the state of another Component of the
    /// same type
    fn mirror(&mut self, other: &dyn Replicate);
    /// Mirror a SINGLE Property field from `other` into `self`, identified
    /// by its 0-based property index (the same index used by the diff-mask
    /// bit positions). Calls `Property::mirror` on exactly one field —
    /// fires that field's PropertyMutator without touching any others.
    ///
    /// Used by the Replicated Resources Mode B mirror system to propagate
    /// per-field changes from the user-facing bevy `Resource` storage to
    /// the entity-component without over-replicating untouched fields.
    ///
    /// **Out-of-range indices are silently no-op'd** (schema evolution
    /// across protocol versions may produce stale dirty indices; we
    /// tolerate that without panicking).
    ///
    /// **Type mismatch** (`other` is not the same concrete type as
    /// `self`) is a programming error: the derive-macro impl
    /// `debug_assert!`s in debug builds and silently no-ops in release.
    /// This is hostile to ignore but a hot per-tick sync system shouldn't
    /// panic in production.
    fn mirror_single_field(&mut self, field_index: u8, other: &dyn Replicate);
    /// Set the Component's PropertyMutator, which keeps track
    /// of which Properties have been mutated, necessary to sync only the
    /// Properties that have changed with the client
    fn set_mutator(&mut self, mutator: &PropertyMutator);
    /// Writes data into an outgoing byte stream, sufficient to completely
    /// recreate the Component on the client
    fn write(
        &self,
        component_kinds: &ComponentKinds,
        writer: &mut dyn BitWrite,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    );
    /// Write data into an outgoing byte stream, sufficient only to update the
    /// mutated Properties of the Component on the client
    fn write_update(
        &self,
        diff_mask: &DiffMask,
        writer: &mut dyn BitWrite,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    );
    /// Reads data from an incoming packet, sufficient to sync the in-memory
    /// Component with it's replica on the Server
    fn read_apply_update(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        update: PendingComponentUpdate,
    ) -> Result<(), SerdeErr>;
    /// Applies a single-field update from the network, updating the corresponding property in-place.
    fn read_apply_field_update(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        update: ComponentFieldUpdate,
    ) -> Result<(), SerdeErr>;
    /// Returns a list of LocalEntities contained within the Component's EntityProperty fields, which are waiting to be converted to GlobalEntities
    fn relations_waiting(&self) -> Option<HashSet<RemoteEntity>>;
    /// Converts any LocalEntities contained within the Component's EntityProperty fields to GlobalEntities
    fn relations_complete(&mut self, converter: &dyn LocalEntityAndGlobalEntityConverter);
    /// Publish Replicate
    fn publish(&mut self, mutator: &PropertyMutator);
    /// Unpublish Replicate
    fn unpublish(&mut self);
    /// Enable Delegation Replicate
    fn enable_delegation(
        &mut self,
        accessor: &EntityAuthAccessor,
        mutator_opt: Option<&PropertyMutator>,
    );
    /// Disable Delegation Replicate
    fn disable_delegation(&mut self);
    /// Convert to Local Replicate
    fn localize(&mut self);
}

/// Marker trait for types that can be stored as a component in a host world.
/// Implemented per-type by the `#[derive(Replicate)]` macro (unconditionally),
/// so every replicated type satisfies this bound without any blanket impl.
/// The Bevy adapter (`naia-bevy-shared`) gates its `ComponentAccessor<R>` on
/// `R: bevy_ecs::component::Component`; `HostComponent` is the naia-owned
/// stand-in that lets core machinery reference the bound without depending on Bevy.
pub trait HostComponent: 'static {}

/// Marker trait combining `Replicate` with `HostComponent`; auto-implemented for all
/// `Replicate + HostComponent` types.
pub trait ReplicatedComponent: Replicate + HostComponent {}
impl<T: Replicate + HostComponent> ReplicatedComponent for T {}
