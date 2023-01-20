/// The maximum of bytes that can be used for the payload of a given packet.
/// (See #38 of <http://ithare.com/64-network-dos-and-donts-for-game-engines-part-v-udp/>)
pub use naia_serde::MTU_SIZE_BYTES;
pub use naia_serde::MTU_SIZE_BITS;

// Number of messages to keep in tick buffer
pub const MESSAGE_HISTORY_SIZE: u16 = 64;
