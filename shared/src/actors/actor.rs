use std::{
    any::TypeId,
    fmt::{Debug, Formatter, Result},
};

use super::{actor_mutator::ActorMutator, actor_type::ActorType, state_mask::StateMask};

use crate::{PacketReader, Ref};

/// An Actor is a container of Properties that can be scoped, tracked, and
/// synced, with a remote host
pub trait Actor<T: ActorType> {
    /// Gets the number of bytes of the Actor's State Mask
    fn get_state_mask_size(&self) -> u8;
    /// Gets a copy of the Actor, wrapped in an ActorType enum (which is the
    /// common protocol between the server/host)
    fn get_typed_copy(&self) -> T;
    /// Gets the TypeId of the Actor's implementation, used to map to a
    /// registered ActorType
    fn get_type_id(&self) -> TypeId;
    /// Writes data into an outgoing byte stream, sufficient to completely
    /// recreate the Actor on the client
    fn write(&self, out_bytes: &mut Vec<u8>);
    /// Write data into an outgoing byte stream, sufficient only to update the
    /// mutated Properties of the Actor on the client
    fn write_partial(&self, state_mask: &StateMask, out_bytes: &mut Vec<u8>);
    /// Reads data from an incoming packet, sufficient to sync the in-memory
    /// Actor with it's state on the Server
    fn read_full(&mut self, reader: &mut PacketReader, packet_index: u16);
    /// Reads data from an incoming packet, sufficient to sync the in-memory
    /// Actor with it's state on the Server
    fn read_partial(
        &mut self,
        state_mask: &StateMask,
        reader: &mut PacketReader,
        packet_index: u16,
    );
    /// Set the Actor's ActorMutator, which keeps track of which Properties
    /// have been mutated, necessary to sync only the Properties that have
    /// changed with the client
    fn set_mutator(&mut self, mutator: &Ref<dyn ActorMutator>);
}

//TODO: do we really need another trait here?
/// Handles equality of Actors.. can't just derive PartialEq because we want
/// to only compare Properties
pub trait ActorEq<T: ActorType, Impl = Self>: Actor<T> {
    /// Compare properties in another Actor
    fn equals(&self, other: &Impl) -> bool;
    /// Sets the current Actor to the state of another Actor of the same type
    fn mirror(&mut self, other: &Impl);
}

impl<T: ActorType> Debug for dyn Actor<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("Actor")
    }
}
