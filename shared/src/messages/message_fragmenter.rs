use naia_serde::{BitWrite, BitWriter};

use crate::{
    constants::{FRAGMENTATION_LIMIT_BITS, FRAGMENTATION_LIMIT_BYTES},
    MessageContainer,
};

// Yeah this is a terrible name
pub struct MessageFragmenter;

impl MessageFragmenter {
    pub fn fragment(message: MessageContainer) -> Vec<MessageContainer> {
        todo!();
    }
}

// BitFragmenter
pub struct BitFragmenter {
    fragments: Vec<(usize, [u8; FRAGMENTATION_LIMIT_BYTES])>,
    current_writer: BitWriter,
}

impl BitFragmenter {
    pub fn new() -> Self {
        Self {
            fragments: Vec::new(),
            current_writer: BitWriter::with_capacity(FRAGMENTATION_LIMIT_BITS),
        }
    }
}

impl BitWrite for BitFragmenter {
    fn write_bit(&mut self, bit: bool) {
        self.current_writer.write_bit(bit);
    }

    fn write_byte(&mut self, byte: u8) {
        todo!()
    }

    fn write_bits(&mut self, bits: u32) {
        panic!("This method only to be used by BitCounter");
    }

    fn is_counter(&self) -> bool {
        false
    }
}
