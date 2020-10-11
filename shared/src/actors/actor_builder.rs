use super::actor_type::ActorType;

use crate::packet_reader::PacketReader;

use std::{
    any::TypeId,
    fmt::{Debug, Formatter, Result},
};

/// Handles the creation of new Actor instances
pub trait ActorBuilder<T: ActorType> {
    /// Create a new Actor instance
    fn build(&self, reader: &mut PacketReader) -> T;
    /// Gets the TypeId of the Actor the builder is able to build
    fn get_type_id(&self) -> TypeId;
}

impl<T: ActorType> Debug for Box<dyn ActorBuilder<T>> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("Boxed ActorBuilder")
    }
}
