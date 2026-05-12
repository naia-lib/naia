use std::{any::Any, collections::HashSet};

use naia_serde::{BitReader, BitWrite, SerdeErr};

use crate::world::update::component_update::ComponentUpdate;
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

pub type SplitUpdateResult = Result<
    (
        Option<Vec<(RemoteEntity, ComponentFieldUpdate)>>,
        Option<ComponentUpdate>,
    ),
    SerdeErr,
>;

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
    fn read_create_update(&self, reader: &mut BitReader) -> Result<ComponentUpdate, SerdeErr>;
    /// Split a Component update into Waiting and Ready updates
    fn split_update(
        &self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        update: ComponentUpdate,
    ) -> SplitUpdateResult;

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
    /// Gets the ComponentKind of this type
    fn kind(&self) -> ComponentKind;
    fn to_any(&self) -> &dyn Any;
    fn to_any_mut(&mut self) -> &mut dyn Any;
    fn to_boxed_any(self: Box<Self>) -> Box<dyn Any>;
    fn copy_to_box(&self) -> Box<dyn Replicate>;
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
        update: ComponentUpdate,
    ) -> Result<(), SerdeErr>;
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

cfg_if! {
    if #[cfg(feature = "bevy_support")]
    {
        // Require that Bevy Component to be implemented
        use bevy_ecs::component::{Component, Mutable};

        pub trait ReplicatedComponent: Replicate + Component<Mutability = Mutable> {}
        impl<T: Replicate + Component<Mutability = Mutable>> ReplicatedComponent for T {}
    }
    else
    {
        pub trait ReplicatedComponent: Replicate {}
        impl<T: Replicate> ReplicatedComponent for T {}
    }
}
