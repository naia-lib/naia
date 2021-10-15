use std::any::TypeId;

use naia_socket_shared::PacketReader;

use crate::EntityType;

use super::{diff_mask::DiffMask, replicate::Replicate};

/// An Enum with a variant for every Component/Message that can be sent
/// between Client/Host
pub trait ProtocolType: Clone + Sync + Send + 'static {
    // write & get_type_id are ONLY currently used for reading/writing auth
    // messages.. maybe should do something different here
    /// Writes the typed Component/Message into an outgoing byte stream
    fn write(&self, buffer: &mut Vec<u8>);
    /// Get the TypeId of the contained Replicate impl
    fn get_type_id(&self) -> TypeId;
    /// Read bytes from an incoming packet into all contained Properties
    fn read_full(&mut self, reader: &mut PacketReader, packet_index: u16);
    /// Read bytes from an incoming packet, updating the Properties which have
    /// been mutated on the Server
    fn read_partial(&mut self, diff_mask: &DiffMask, reader: &mut PacketReader, packet_index: u16);
    /// Compare properties in another ProtocolType
    fn equals(&self, other: &Self) -> bool;
    /// Sets the current Component/Message to the state of another of the
    /// same type
    fn mirror(&mut self, other: &Self);
    /// Creates a copy of self, different than clone (which works as a smart
    /// reference)
    fn copy(&self) -> Self;
    /// Get an immutable reference to the inner Component/Message as a Replicate trait object
    fn dyn_ref(&self) -> &dyn Replicate<Self>;
    /// Get an mutable reference to the inner Component/Message as a Replicate trait object
    fn dyn_mut(&mut self) -> &mut dyn Replicate<Self>;
    /// Cast to a typed immutable reference to the inner Component/Message
    fn cast_ref<R: Replicate<Self>>(&self) -> Option<&R>;
    /// Cast to a typed mutable reference to the inner Component/Message
    fn cast_mut<R: Replicate<Self>>(&mut self) -> Option<&mut R>;
    /// Extract an inner typed Ref from within the ProtocolType, into a
    /// ProtocolExtractor impl
    fn extract_and_insert<K: EntityType, E: ProtocolExtractor<Self, K>>(
        &self,
        key: &K,
        extractor: &mut E,
    );
}

pub trait ProtocolExtractor<P: ProtocolType, K: EntityType> {
    fn extract<R: Replicate<P>>(&mut self, entity: &K, inner: R);
}

pub trait ProtocolKindType {}