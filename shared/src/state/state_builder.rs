use super::state_type::StateType;

use crate::PacketReader;

use std::{
    any::TypeId,
    fmt::{Debug, Formatter, Result},
};

/// Handles the creation of new State instances
pub trait StateBuilder<T: StateType> {
    /// Create a new State instance
    fn state_build(&self, reader: &mut PacketReader) -> T;
    /// Gets the TypeId of the State the builder is able to build
    fn get_type_id(&self) -> TypeId;
}

impl<T: StateType> Debug for Box<dyn StateBuilder<T>> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("Boxed StateBuilder")
    }
}
