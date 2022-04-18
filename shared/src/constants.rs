/// The maximum of bytes that can be used for the payload of a given packet.
/// (See #38 of http://ithare.com/64-network-dos-and-donts-for-game-engines-part-v-udp/)
pub const MTU_SIZE_BYTES: u16 = 508;
pub const MTU_SIZE_BITS: u16 = MTU_SIZE_BYTES * 8;

// Number of messages to keep in tick buffer
pub const MESSAGE_HISTORY_SIZE: u16 = 64;
