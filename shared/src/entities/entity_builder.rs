use super::entity_type::EntityType;

use crate::packet_reader::PacketReader;

use std::{
    any::TypeId,
    fmt::{Debug, Formatter, Result},
};

/// Handles the creation of new Entity instances
pub trait EntityBuilder<T: EntityType> {
    /// Create a new Entity instance
    fn build(&self, reader: &mut PacketReader) -> T;
    /// Gets the TypeId of the Entity the builder is able to build
    fn get_type_id(&self) -> TypeId;
}

impl<T: EntityType> Debug for Box<dyn EntityBuilder<T>> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("Boxed EntityBuilder")
    }
}
