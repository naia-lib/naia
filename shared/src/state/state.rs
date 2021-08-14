use std::{
    any::TypeId,
    fmt::{Debug, Formatter, Result},
};

use super::{state_mutator::StateMutator, state_type::StateType, diff_mask::DiffMask};

use crate::{PacketReader, Ref};

/// An State is a container of Properties that can be scoped, tracked, and
/// synced, with a remote host
pub trait State<T: StateType>: EventClone<T> {
    /// Whether the Event is guaranteed for eventual delivery to the remote
    /// host.
    fn event_is_guaranteed(&self) -> bool;
    /// Writes the current Event into an outgoing packet's byte stream
    fn event_write(&self, out_bytes: &mut Vec<u8>);
    /// Gets a copy of the Event, encapsulated within an EventType enum
    fn event_get_typed_copy(&self) -> T;
    /// Gets the TypeId of the Event
    fn event_get_type_id(&self) -> TypeId;
    /// Gets the number of bytes of the State's State Mask
    fn state_get_diff_mask_size(&self) -> u8;
    /// Gets a copy of the State, wrapped in an StateType enum (which is the
    /// common protocol between the server/host)
    fn state_get_typed_copy(&self) -> T;
    /// Gets the TypeId of the State's implementation, used to map to a
    /// registered StateType
    fn state_get_type_id(&self) -> TypeId;
    /// Writes data into an outgoing byte stream, sufficient to completely
    /// recreate the State on the client
    fn state_write(&self, out_bytes: &mut Vec<u8>);
    /// Write data into an outgoing byte stream, sufficient only to update the
    /// mutated Properties of the State on the client
    fn state_write_partial(&self, diff_mask: &DiffMask, out_bytes: &mut Vec<u8>);
    /// Reads data from an incoming packet, sufficient to sync the in-memory
    /// State with it's state on the Server
    fn state_read_full(&mut self, reader: &mut PacketReader, packet_index: u16);
    /// Reads data from an incoming packet, sufficient to sync the in-memory
    /// State with it's state on the Server
    fn state_read_partial(
        &mut self,
        diff_mask: &DiffMask,
        reader: &mut PacketReader,
        packet_index: u16,
    );
    /// Set the State's StateMutator, which keeps track of which Properties
    /// have been mutated, necessary to sync only the Properties that have
    /// changed with the client
    fn state_set_mutator(&mut self, mutator: &Ref<dyn StateMutator>);
}

//TODO: do we really need another trait here?
/// Handles equality of States.. can't just derive PartialEq because we want
/// to only compare Properties
pub trait StateEq<T: StateType, Impl = Self>: State<T> {
    /// Compare properties in another State
    fn state_equals(&self, other: &Impl) -> bool;
    /// Sets the current State to the state of another State of the same type
    fn state_mirror(&mut self, other: &Impl);
}

impl<T: StateType> Debug for dyn State<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("State")
    }
}

/// A Boxed Event must be able to clone itself
pub trait EventClone<T: StateType> {
    /// Clone the Boxed Event
    fn event_clone_box(&self) -> Box<dyn State<T>>;
}

impl<Z: StateType, T: 'static + State<Z> + Clone> EventClone<Z> for T {
    fn event_clone_box(&self) -> Box<dyn State<Z>> {
        Box::new(self.clone())
    }
}

impl<T: StateType> Clone for Box<dyn State<T>> {
    fn clone(&self) -> Box<dyn State<T>> {
        EventClone::event_clone_box(self.as_ref())
    }
}

//impl<T: StateType> Debug for Box<dyn State<T>> {
//    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
//        f.write_str("Boxed Event")
//    }
//}
