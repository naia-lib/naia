use crate::PacketIndex;

pub struct PacketWriteState {
    pub packet_index: PacketIndex,
    bytes: usize,
}

impl PacketWriteState {
    pub fn new(next_packet_index: PacketIndex) -> Self {
        PacketWriteState {
            packet_index: next_packet_index,
            bytes: 0,
        }
    }

    pub fn byte_count(&self) -> usize {
        self.bytes
    }

    pub fn add_bytes(&mut self, is_initial: bool, initial_bytes: usize, total_bytes: usize) {
        if is_initial {
            self.bytes += initial_bytes;
        }

        self.bytes += total_bytes;
    }
}
