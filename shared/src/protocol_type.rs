use std::any::TypeId;

use super::{diff_mask::DiffMask, replicate::Replicate};

use crate::{PacketReader, Ref};

/// An Enum with a variant for every Object/Component/Message that can be sent
/// between Client/Host
pub trait ProtocolType<Impl = Self>: Clone {
    // write & get_type_id are ONLY currently used for reading/writing auth
    // messages.. maybe should do something different here
    /// Writes the typed Object/Component/Message into an outgoing byte stream
    fn write(&self, buffer: &mut Vec<u8>);
    /// Get the TypeId of the contained Object/Component/Message
    fn get_type_id(&self) -> TypeId;
    /// Read bytes from an incoming packet into all contained Properties
    fn read_full(&mut self, reader: &mut PacketReader, packet_index: u16);
    /// Read bytes from an incoming packet, updating the Properties which have
    /// been mutated on the Server
    fn read_partial(&mut self, diff_mask: &DiffMask, reader: &mut PacketReader, packet_index: u16);
    /// Convert ProtocolType to an inner reference of the
    /// Object/Component/Message
    fn inner_ref(&self) -> Ref<dyn Replicate<Impl>>;
    /// Compare properties in another ProtocolType
    fn equals(&self, other: &Impl) -> bool;
    /// Sets the current Object/Component/Message to the state of another of the
    /// same type
    fn mirror(&mut self, other: &Impl);
}
