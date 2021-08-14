use std::{
    any::TypeId,
    fmt::{Debug, Formatter, Result},
};

use crate::{PacketReader, StateType};

/// Handles the creation of new Events
pub trait EventBuilder<T: StateType> {
    /// Creates a new Event
    fn event_build(&self, reader: &mut PacketReader) -> T;
    /// Gets the TypeId of the Event it is able to build
    fn event_get_type_id(&self) -> TypeId;
}

impl<T: StateType> Debug for Box<dyn EventBuilder<T>> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("Boxed EventBuilder")
    }
}
