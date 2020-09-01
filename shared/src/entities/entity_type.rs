use std::{cell::RefCell, rc::Rc};

use super::{entity::Entity, state_mask::StateMask};

/// An Enum with a variant for every Entity that can be synced between
/// Client/Host
pub trait EntityType<Impl = Self>: Clone {
    /// Read bytes from an incoming packet, updating the Properties which have
    /// been mutated on the Server
    fn read_partial(&mut self, state_mask: &StateMask, bytes: &[u8], packet_index: u16);
    /// Convert EntityType to an inner reference to the Entity
    fn inner_ref(&self) -> Rc<RefCell<dyn Entity<Impl>>>;
}
