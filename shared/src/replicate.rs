use std::{
    any::{Any, TypeId},
    fmt::{Debug, Formatter, Result},
};

use naia_socket_shared::{PacketReader, Ref};

use super::{diff_mask::DiffMask, property_mutate::PropertyMutate, protocol_type::ProtocolType};

/// A Replica is a Message/Component, or otherwise, a container
/// of Properties that can be scoped, tracked, and synced, with a remote host
pub trait Replicate<P: ProtocolType>: Any + Sync + Send + 'static {
    /// Gets the number of bytes of the Message/Component's DiffMask
    fn get_diff_mask_size(&self) -> u8;
    /// Gets the TypeId of the Message/Component, used to map to a
    /// registered ProtocolType
    fn get_type_id(&self) -> TypeId;
    /// Writes data into an outgoing byte stream, sufficient to completely
    /// recreate the Message/Component on the client
    fn write(&self, out_bytes: &mut Vec<u8>);
    /// Write data into an outgoing byte stream, sufficient only to update the
    /// mutated Properties of the Message/Component on the client
    fn write_partial(&self, diff_mask: &DiffMask, out_bytes: &mut Vec<u8>);
    /// Reads data from an incoming packet, sufficient to sync the in-memory
    /// Message/Component with it's replica on the Server
    fn read_full(&mut self, reader: &mut PacketReader, packet_index: u16);
    /// Reads data from an incoming packet, sufficient to sync the in-memory
    /// Component with it's replica on the Server
    fn read_partial(&mut self, diff_mask: &DiffMask, reader: &mut PacketReader, packet_index: u16);
    /// Set the Message/Component's PropertyMutator, which keeps track
    /// of which Properties have been mutated, necessary to sync only the
    /// Properties that have changed with the client
    fn set_mutator(&mut self, mutator: &Ref<dyn PropertyMutate>);
    /// Returns self
    fn as_protocol(&self) -> P;
}

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

impl<P: ProtocolType> Debug for dyn Replicate<P> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("Replicate")
    }
}
