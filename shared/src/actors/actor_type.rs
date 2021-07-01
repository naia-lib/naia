use super::{actor::Actor, state_mask::StateMask};

use crate::{PacketReader, Ref};

/// An Enum with a variant for every Actor that can be synced between
/// Client/Host
pub trait ActorType<Impl = Self>: Clone {
    /// Read bytes from an incoming packet into all contained Properties
    fn read_full(&mut self, reader: &mut PacketReader, packet_index: u16);
    /// Read bytes from an incoming packet, updating the Properties which have
    /// been mutated on the Server
    fn read_partial(
        &mut self,
        state_mask: &StateMask,
        reader: &mut PacketReader,
        packet_index: u16,
    );
    /// Convert ActorType to an inner reference to the Actor
    fn inner_ref(&self) -> Ref<dyn Actor<Impl>>;
    /// Compare properties in another ActorType
    fn equals(&self, other: &Impl) -> bool;
    /// Compare predicted properties in another ActorType
    fn equals_prediction(&self, other: &Impl) -> bool;
    /// Sets the current Actor to the state of another Actor of the same type
    fn mirror(&mut self, other: &Impl);
    /// Returns whether or not the Actor has any predicted properties
    fn is_predicted(&self) -> bool;
}
