use std::{any::Any, collections::HashSet};

use naia_serde::{BitReader, BitWrite, SerdeErr};

use crate::{
    messages::named::Named,
    world::{
        component::{
            component_kinds::{ComponentKind, ComponentKinds},
            component_update::ComponentUpdate,
            diff_mask::DiffMask,
            property_mutate::PropertyMutator,
            replica_ref::{ReplicaDynMut, ReplicaDynRef},
        },
        entity::entity_converters::LocalEntityAndGlobalEntityConverter,
    },
    ComponentFieldUpdate, LocalEntity, LocalEntityAndGlobalEntityConverterMut,
};

pub trait ReplicateBuilder: Send + Sync + Named {
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
    ) -> Result<
        (
            Option<Vec<(LocalEntity, ComponentFieldUpdate)>>,
            Option<ComponentUpdate>,
        ),
        SerdeErr,
    >;
}

/// A struct that implements Replicate is a Component, or otherwise,
/// a container of Properties that can be scoped, tracked, and synced, with a
/// remote host
pub trait Replicate: ReplicateInner + Named + Any {
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
    fn relations_waiting(&self) -> Option<HashSet<LocalEntity>>;
    /// Converts any LocalEntities contained within the Component's EntityProperty fields to GlobalEntities
    fn relations_complete(&mut self, converter: &dyn LocalEntityAndGlobalEntityConverter);
    /// Publish Replicate
    fn publish(&mut self, mutator: &PropertyMutator);
    /// Unpublish Replicate
    fn unpublish(&mut self);
    /// Enable Delegation Replicate
    fn enable_delegation(&mut self);
    /// Disable Delegation Replicate
    fn disable_delegation(&mut self);
    /// Convert to Local Replicate
    fn localize(&mut self);
}

cfg_if! {
    if #[cfg(feature = "bevy_support")]
    {
        // Require that Bevy Component to be implemented
        use bevy_ecs::component::{TableStorage, Component};

        pub trait ReplicateInner: Component<Storage = TableStorage> + Sync + Send + 'static {}

        impl<T> ReplicateInner for T
        where T: Component<Storage = TableStorage> + Sync + Send + 'static {
        }
    }
    else
    {
        pub trait ReplicateInner: Sync + Send + 'static {}

        impl<T> ReplicateInner for T
        where T: Sync + Send + 'static {
        }
    }
}
