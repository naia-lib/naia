use super::{state::State, diff_mask::DiffMask};

use crate::{PacketReader, Ref};

/// An Enum with a variant for every State that can be synced between
/// Client/Host
pub trait StateType<Impl = Self>: Clone {
    /// Read bytes from an incoming packet into all contained Properties
    fn read_full(&mut self, reader: &mut PacketReader, packet_index: u16);
    /// Read bytes from an incoming packet, updating the Properties which have
    /// been mutated on the Server
    fn read_partial(
        &mut self,
        diff_mask: &DiffMask,
        reader: &mut PacketReader,
        packet_index: u16,
    );
    /// Convert StateType to an inner reference to the State
    fn inner_ref(&self) -> Ref<dyn State<Impl>>;
    /// Compare properties in another StateType
    fn equals(&self, other: &Impl) -> bool;
    /// Sets the current State to the state of another State of the same type
    fn mirror(&mut self, other: &Impl);
}
