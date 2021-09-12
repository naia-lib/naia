use std::any::TypeId;

use super::{diff_mask::DiffMask, replicate::Replicate};

use crate::{PacketReader, Ref};

/// An Enum with a variant for every Component/Message that can be sent
/// between Client/Host
pub trait ProtocolType<Impl = Self>: Clone
where
    Impl: ProtocolType,
{
    // write & get_type_id are ONLY currently used for reading/writing auth
    // messages.. maybe should do something different here
    /// Writes the typed Component/Message into an outgoing byte stream
    fn write(&self, buffer: &mut Vec<u8>);
    /// Get the TypeId of the contained Component/Message
    fn get_type_id(&self) -> TypeId;
    /// Read bytes from an incoming packet into all contained Properties
    fn read_full(&mut self, reader: &mut PacketReader, packet_index: u16);
    /// Read bytes from an incoming packet, updating the Properties which have
    /// been mutated on the Server
    fn read_partial(&mut self, diff_mask: &DiffMask, reader: &mut PacketReader, packet_index: u16);
    /// Convert ProtocolType to an inner reference of the
    /// Component/Message
    fn inner_ref(&self) -> Ref<dyn Replicate<Impl>>;
    /// Convert ProtocolType to a typed inner reference of the
    /// Component/Message
    fn to_typed_ref<T: Replicate<Impl>>(&self) -> Option<Ref<T>>;
    /// Convert ProtocolType to a typed inner reference of the
    /// Component/Message
    fn as_typed_ref<T: Replicate<Impl>>(&self) -> Option<&Ref<T>>;
    /// Compare properties in another ProtocolType
    fn equals(&self, other: &Impl) -> bool;
    /// Sets the current Component/Message to the state of another of the
    /// same type
    fn mirror(&mut self, other: &Impl);
    /// Creates a copy of self, different than clone (which works as a smart
    /// reference)
    fn copy(&self) -> Impl;
}
