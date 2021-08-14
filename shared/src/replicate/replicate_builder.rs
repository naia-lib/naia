use super::protocol_type::ProtocolType;

use crate::PacketReader;

use std::{
    any::TypeId,
    fmt::{Debug, Formatter, Result},
};

/// Handles the creation of new Replicate instances
pub trait ReplicateBuilder<T: ProtocolType> {
    /// Create a new Replicate instance
    fn build(&self, reader: &mut PacketReader) -> T;
    /// Gets the TypeId of the Replicate the builder is able to build
    fn get_type_id(&self) -> TypeId;
}

impl<T: ProtocolType> Debug for Box<dyn ReplicateBuilder<T>> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("Boxed ReplicateBuilder")
    }
}
