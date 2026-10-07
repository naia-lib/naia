use crate::BitWrite;

// BitCounter
/// A [`BitWrite`] implementation that only tallies bits instead of writing
/// them, used to measure how many bits a value would take before committing
/// it to a real writer.
pub struct BitCounter {
    start_bits: u32,
    current_bits: u32,
    max_bits: u32,
}

impl BitCounter {
    /// Creates a counter starting at `start_bits`/`current_bits` already
    /// tallied, capped at `max_bits`.
    pub fn new(start_bits: u32, current_bits: u32, max_bits: u32) -> Self {
        Self {
            start_bits,
            current_bits,
            max_bits,
        }
    }

    /// True once the tallied bit count has passed `max_bits`.
    pub fn overflowed(&self) -> bool {
        self.current_bits > self.max_bits
    }

    /// Bits tallied since this counter was created (`current_bits - start_bits`).
    pub fn bits_needed(&self) -> u32 {
        self.current_bits - self.start_bits
    }
}

impl BitWrite for BitCounter {
    fn write_bit(&mut self, _: bool) {
        self.current_bits += 1;
    }
    fn write_byte(&mut self, _: u8) {
        self.current_bits += 8;
    }
    fn count_bits(&mut self, bits: u32) {
        self.current_bits += bits;
    }
    fn is_counter(&self) -> bool {
        true
    }
}
