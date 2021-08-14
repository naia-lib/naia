use std::any::TypeId;

use super::{replicate::Replicate, diff_mask::DiffMask};

use crate::{PacketReader, Ref};

/// An Enum with a variant for every Replicate that can be synced between
/// Client/Host
pub trait ProtocolType<Impl = Self>: Clone {
    // event_write & get_type_id are ONLY currently used for reading/writing auth events..
    // maybe should do something different here
    /// Writes the typed Event into an outgoing byte stream
    fn write(&self, buffer: &mut Vec<u8>);
    /// Get the TypeId of the contained Event
    fn get_type_id(&self) -> TypeId;
    /// Read bytes from an incoming packet into all contained Properties
    fn read_full(&mut self, reader: &mut PacketReader, packet_index: u16);
    /// Read bytes from an incoming packet, updating the Properties which have
    /// been mutated on the Server
    fn read_partial(
        &mut self,
        diff_mask: &DiffMask,
        reader: &mut PacketReader,
        packet_index: u16,
    );
    /// Convert ProtocolType to an inner reference to the Replicate
    fn inner_ref(&self) -> Ref<dyn Replicate<Impl>>;
    /// Compare properties in another ProtocolType
    fn equals(&self, other: &Impl) -> bool;
    /// Sets the current Replicate to the replicate of another Replicate of the same type
    fn mirror(&mut self, other: &Impl);
}
