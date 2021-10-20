use naia_socket_shared::PacketReader;

use crate::{diff_mask::DiffMask, property_mutate::PropertyMutator, protocol_type::{ProtocolType, DynRef, DynMut}};

/// A Replica is a Message/Component, or otherwise, a container
/// of Properties that can be scoped, tracked, and synced, with a remote host
pub trait Replicate<P: ProtocolType>: Sync + Send + 'static {
    /// Gets the TypeId of the Message/Component, used to map to a
    /// registered ProtocolType
    fn get_kind(&self) -> P::Kind;
    /// Gets the number of bytes of the Message/Component's DiffMask
    fn get_diff_mask_size(&self) -> u8;
    /// Returns self as a Protocol
    fn to_protocol(self) -> P;
    /// Set the Message/Component's PropertyMutator, which keeps track
    /// of which Properties have been mutated, necessary to sync only the
    /// Properties that have changed with the client
    fn set_mutator(&mut self, mutator: &PropertyMutator);
    /// Writes data into an outgoing byte stream, sufficient to completely
    /// recreate the Message/Component on the client
    fn write(&self, out_bytes: &mut Vec<u8>);
    /// Write data into an outgoing byte stream, sufficient only to update the
    /// mutated Properties of the Message/Component on the client
    fn write_partial(&self, diff_mask: &DiffMask, out_bytes: &mut Vec<u8>);
    /// Reads data from an incoming packet, sufficient to sync the in-memory
    /// Component with it's replica on the Server
    fn read_partial(&mut self, diff_mask: &DiffMask, reader: &mut PacketReader, packet_index: u16);
    /// Get an immutable reference to the inner Component/Message as a Replicate
    /// trait object
    fn dyn_ref(&self) -> DynRef<'_, P>;
    /// Get an mutable reference to the inner Component/Message as a Replicate
    /// trait object
    fn dyn_mut(&mut self) -> DynMut<'_, P>;
}

//pub trait ReplicateRef<P: ProtocolType, R: ProtocolRefType<P>>: Sync + Send {
//    fn to_protocol_ref(self) -> R;
//}
//
//pub trait ReplicateMut<P: ProtocolType, R: ProtocolMutType<P>>: Sync + Send {
//    fn to_protocol_mut(self) -> R;
//}

//TODO: do we really need another trait here?
/// Handles equality of Messages/Components.. can't just derive
/// PartialEq because we want to only compare Properties
pub trait ReplicateEq<P: ProtocolType>: Replicate<P> {
    /// Compare with properties in another Replica
    fn equals(&self, other: &Self) -> bool;
    /// Sets the current Replica to the state of another Replica of the
    /// same type
    fn mirror(&mut self, other: &Self);
    /// Gets a copy of the Message/Component
    fn copy(&self) -> Self;
}
