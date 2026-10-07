use crate::MTU_SIZE_BYTES;

/// A finalized packet payload: a fixed MTU-sized buffer plus the number of
/// bytes actually written into it.
pub struct OutgoingPacket {
    payload_length: usize,
    payload: [u8; MTU_SIZE_BYTES],
}

impl OutgoingPacket {
    /// Wraps a payload buffer together with how many of its bytes are valid.
    pub fn new(payload_length: usize, payload: [u8; MTU_SIZE_BYTES]) -> Self {
        Self {
            payload_length,
            payload,
        }
    }

    /// The written bytes, excluding the unused tail of the fixed buffer.
    pub fn slice(&self) -> &[u8] {
        &self.payload[0..self.payload_length]
    }
}
