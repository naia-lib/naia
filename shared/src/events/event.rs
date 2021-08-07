use std::{
    any::TypeId,
    fmt::{Debug, Formatter, Result},
};

use super::event_type::EventType;

/// An Event is a struct of data that can be sent and recreated on the connected
/// remote host
pub trait Event<T: EventType>: EventClone<T> {
    /// Whether the Event is guaranteed for eventual delivery to the remote
    /// host.
    fn event_is_guaranteed(&self) -> bool;
    /// Writes the current Event into an outgoing packet's byte stream
    fn event_write(&self, out_bytes: &mut Vec<u8>);
    /// Gets a copy of the Event, encapsulated within an EventType enum
    fn event_get_typed_copy(&self) -> T;
    /// Gets the TypeId of the Event
    fn event_get_type_id(&self) -> TypeId;
}

/// A Boxed Event must be able to clone itself
pub trait EventClone<T: EventType> {
    /// Clone the Boxed Event
    fn event_clone_box(&self) -> Box<dyn Event<T>>;
}

impl<Z: EventType, T: 'static + Event<Z> + Clone> EventClone<Z> for T {
    fn event_clone_box(&self) -> Box<dyn Event<Z>> {
        Box::new(self.clone())
    }
}

impl<T: EventType> Clone for Box<dyn Event<T>> {
    fn clone(&self) -> Box<dyn Event<T>> {
        EventClone::event_clone_box(self.as_ref())
    }
}

impl<T: EventType> Debug for Box<dyn Event<T>> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("Boxed Event")
    }
}
