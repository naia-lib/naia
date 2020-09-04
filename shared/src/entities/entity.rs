use std::{
    any::TypeId,
    cell::RefCell,
    fmt::{Debug, Formatter, Result},
    rc::Rc,
};

use super::{entity_mutator::EntityMutator, entity_type::EntityType, state_mask::StateMask};

/// An Entity is a container of Properties that can be scoped, tracked, and
/// synced, with a remote host
pub trait Entity<T: EntityType> {
    /// Gets the number of bytes of the Entity's State Mask
    fn get_state_mask_size(&self) -> u8;
    /// Gets a copy of the Entity, wrapped in an EntityType enum (which is the
    /// common protocol between the server/host)
    fn get_typed_copy(&self) -> T;
    /// Gets the TypeId of the Entity's implementation, used to map to a
    /// registered EntityType
    fn get_type_id(&self) -> TypeId;
    /// Writes data into an outgoing byte stream, sufficient to completely
    /// recreate the Entity on the client
    fn write(&self, out_bytes: &mut Vec<u8>);
    /// Write data into an outgoing byte stream, sufficient only to update the
    /// mutated Properties of the Entity on the client
    fn write_partial(&self, state_mask: &StateMask, out_bytes: &mut Vec<u8>);
    /// Reads data from an incoming packet, sufficient to sync the in-memory
    /// Entity with it's state on the Server
    fn read_partial(&mut self, state_mask: &StateMask, in_bytes: &[u8], packet_index: u16);
    /// Set the Entity's EntityMutator, which keeps track of which Properties
    /// have been mutated, necessary to sync only the Properties that have
    /// changed with the client
    fn set_mutator(&mut self, mutator: &Rc<RefCell<dyn EntityMutator>>);
}

/// Handles equality of Entities.. can't just derive PartialEq because we want
/// to only compare Properties
pub trait EntityEq<T: EntityType, Impl = Self>: Entity<T> {
    /// Compare properties in another Entity
    fn equals(&self, other: &Impl) -> bool;
}

impl<T: EntityType> Debug for dyn Entity<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("Entity")
    }
}
