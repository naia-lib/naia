use super::protocol_type::ProtocolType;

use crate::PacketReader;

use std::{
    any::TypeId,
    fmt::{Debug, Formatter, Result},
};

/// Handles the creation of new Replica (Message/Component) instances
pub trait ReplicaBuilder<T: ProtocolType> {
    /// Create a new Replica instance
    fn build(&self, reader: &mut PacketReader) -> T;
    /// Gets the TypeId of the Replica the builder is able to build
    fn get_type_id(&self) -> TypeId;
}

impl<T: ProtocolType> Debug for Box<dyn ReplicaBuilder<T>> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("Boxed ReplicaBuilder")
    }
}
