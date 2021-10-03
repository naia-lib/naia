use std::{
    any::{Any, TypeId},
    fmt::{Debug, Formatter, Result},
};

use naia_socket_shared::{PacketReader, Ref};

use super::{diff_mask::DiffMask, property_mutate::PropertyMutate, protocol_type::ProtocolType};

/// A Replica is a Message/Component, or otherwise, a container
/// of Properties that can be scoped, tracked, and synced, with a remote host
pub trait Replicate<P: ProtocolType>: Any {
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
    /// Copies underlying Replica to a Protocol
    fn copy_to_protocol(&self) -> P;
}

//TODO: do we really need another trait here?
/// Handles equality of Messages/Components.. can't just derive
/// PartialEq because we want to only compare Properties
pub trait ReplicaEq<P: ProtocolType, Impl = Self>: Replicate<P> {
    /// Compare with properties in another Replica
    fn equals(&self, other: &Impl) -> bool;
    /// Sets the current Replica to the state of another Replica of the
    /// same type
    fn mirror(&mut self, other: &Impl);
    /// Gets a copy of the Message/Component
    fn copy(&self) -> Impl;
}

impl<P: ProtocolType> Debug for dyn Replicate<P> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("Replica")
    }
}

/// Represents a Ref of a concrete type that implements Replicate
pub trait ImplRef<P: ProtocolType>: Any + Send + Sync {
    /// Converts the Ref to a ProtocolType
    fn protocol(&self) -> P;
    /// Converts the Ref to a Trait Object Ref
    fn dyn_ref(&self) -> Ref<dyn Replicate<P>>;
    /// Clones the Ref
    fn clone_ref(&self) -> Self;
}
