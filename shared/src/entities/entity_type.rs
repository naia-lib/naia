use super::state_mask::StateMask;

/// An Enum with a variant for every Entity that can be synced between
/// Client/Host
pub trait EntityType {
    /// Read bytes from an incoming packet, updating the Properties which have
    /// been mutated on the Server
    fn read_partial(&mut self, state_mask: &StateMask, bytes: &[u8], packet_index: u16);
}
