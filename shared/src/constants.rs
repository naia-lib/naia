use crate::StandardHeader;

/// The maximum of bytes that can be used for the payload of a given packet.
/// (See #38 of http://ithare.com/64-network-dos-and-donts-for-game-engines-part-v-udp/)
pub const MTU_SIZE: usize = 508 - StandardHeader::bytes_number();
