use std::{
    any::TypeId,
    fmt::{Debug, Formatter, Result},
};

use super::{diff_mask::DiffMask, property_mutate::PropertyMutate, protocol_type::ProtocolType};

use crate::{PacketReader, Ref};

/// A Replica is a Message/Object/Component, or otherwise, a container
/// of Properties that can be scoped, tracked, and synced, with a remote host
pub trait Replicate<T: ProtocolType>: BoxClone<T> {
    /// Gets the number of bytes of the Message/Object/Component's DiffMask
    fn get_diff_mask_size(&self) -> u8;
    /// Gets a copy of the Message/Object/Component, wrapped in an ProtocolType
    /// enum (which is the common protocol between the server/host)
    fn to_protocol(&self) -> T;
    /// Gets the TypeId of the Message/Object/Component, used to map to a
    /// registered ProtocolType
    fn get_type_id(&self) -> TypeId;
    /// Writes data into an outgoing byte stream, sufficient to completely
    /// recreate the Message/Object/Component on the client
    fn write(&self, out_bytes: &mut Vec<u8>);
    /// Write data into an outgoing byte stream, sufficient only to update the
    /// mutated Properties of the Message/Object/Component on the client
    fn write_partial(&self, diff_mask: &DiffMask, out_bytes: &mut Vec<u8>);
    /// Reads data from an incoming packet, sufficient to sync the in-memory
    /// Message/Object/Component with it's replica on the Server
    fn read_full(&mut self, reader: &mut PacketReader, packet_index: u16);
    /// Reads data from an incoming packet, sufficient to sync the in-memory
    /// Message/Object/Component with it's replica on the Server
    fn read_partial(&mut self, diff_mask: &DiffMask, reader: &mut PacketReader, packet_index: u16);
    /// Set the Message/Object/Component's PropertyMutator, which keeps track
    /// of which Properties have been mutated, necessary to sync only the
    /// Properties that have changed with the client
    fn set_mutator(&mut self, mutator: &Ref<dyn PropertyMutate>);
}

//TODO: do we really need another trait here?
/// Handles equality of Messages/Objects/Components.. can't just derive
/// PartialEq because we want to only compare Properties
pub trait ReplicaEq<T: ProtocolType, Impl = Self>: Replicate<T> {
    /// Compare properties in another Replicate
    fn equals(&self, other: &Impl) -> bool;
    /// Sets the current Replica to the state of another Replica of the
    /// same type
    fn mirror(&mut self, other: &Impl);
}

impl<T: ProtocolType> Debug for dyn Replicate<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("Replicate")
    }
}

/// A Boxed Replicate must be able to clone itself
pub trait BoxClone<T: ProtocolType> {
    /// Clone the Boxed Event
    fn box_clone(&self) -> Box<dyn Replicate<T>>;
}

impl<Z: ProtocolType, T: 'static + Replicate<Z> + Clone> BoxClone<Z> for T {
    fn box_clone(&self) -> Box<dyn Replicate<Z>> {
        Box::new(self.clone())
    }
}

impl<T: ProtocolType> Clone for Box<dyn Replicate<T>> {
    fn clone(&self) -> Box<dyn Replicate<T>> {
        BoxClone::box_clone(self.as_ref())
    }
}
