use naia_socket_shared::PacketReader;

use crate::{
    diff_mask::DiffMask,
    property_mutate::PropertyMutator,
    protocolize::Protocolize,
    replica_ref::{ReplicaDynMut, ReplicaDynRef},
};

/// A struct that implements Replicate is a Message/Component, or otherwise,
/// a container of Properties that can be scoped, tracked, and synced, with a
/// remote host
pub trait Replicate<P: Protocolize>: ReplicateSafe<P> {
    /// Returns a clone of self
    fn clone(&self) -> Self;
}

/// The part of Replicate which is object-safe
pub trait ReplicateSafe<P: Protocolize>: ReplicateInner {
    /// Gets the TypeId of the Message/Component, used to map to a
    /// registered Protocolize
    fn kind(&self) -> P::Kind;
    /// Gets the number of bytes of the Message/Component's DiffMask
    fn diff_mask_size(&self) -> u8;
    /// Get an immutable reference to the inner Component/Message as a
    /// Replicate trait object
    fn dyn_ref(&self) -> ReplicaDynRef<'_, P>;
    /// Get an mutable reference to the inner Component/Message as a
    /// Replicate trait object
    fn dyn_mut(&mut self) -> ReplicaDynMut<'_, P>;
    /// Returns self as a Protocol
    fn into_protocol(self) -> P;
    /// Returns a copy of self as a Protocol
    fn protocol_copy(&self) -> P;
    /// Sets the current Replica to the state of another Replica of the
    /// same type
    fn mirror(&mut self, other: &P);
    /// Set the Message/Component's PropertyMutator, which keeps track
    /// of which Properties have been mutated, necessary to sync only the
    /// Properties that have changed with the client
    fn set_mutator(&mut self, mutator: &PropertyMutator);
    /// Reads data from an incoming packet, sufficient to sync the in-memory
    /// Component with it's replica on the Server
    fn read_partial(&mut self, diff_mask: &DiffMask, reader: &mut PacketReader, packet_index: u16);
    /// Writes data into an outgoing byte stream, sufficient to completely
    /// recreate the Message/Component on the client
    fn write(&self, out_bytes: &mut Vec<u8>);
    /// Write data into an outgoing byte stream, sufficient only to update the
    /// mutated Properties of the Message/Component on the client
    fn write_partial(&self, diff_mask: &DiffMask, out_bytes: &mut Vec<u8>);
}

cfg_if! {
    if #[cfg(feature = "bevy_support")]
    {
        // Require that Bevy Component to be implemented
        use bevy::{ecs::component::TableStorage, prelude::Component};

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
