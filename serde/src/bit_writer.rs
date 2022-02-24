
use super::consts::MAX_BUFFER_SIZE;

pub struct BitWriter {
    scratch: u8,
    scratch_index: u8,
    buffer: [u8; MAX_BUFFER_SIZE],
    buffer_index: usize,
}

impl BitWriter {
    pub fn new() -> Self {
        Self {
            scratch: 0,
            scratch_index: 0,
            buffer: [0; MAX_BUFFER_SIZE],
            buffer_index: 0,
        }
    }

    pub fn write_bit(&mut self, bit: bool) {
        self.scratch = self.scratch << 1;

        if bit {
            self.scratch |= 1;
        }

        self.scratch_index += 1;

        if self.scratch_index >= 8 {

            self.buffer[self.buffer_index] = self.scratch.reverse_bits();

            self.buffer_index += 1;
            self.scratch_index -= 8;
            self.scratch = 0;
        }
    }

    pub fn flush(&mut self) -> (usize, [u8; MAX_BUFFER_SIZE]) {

        if self.scratch_index > 0 {
            self.buffer[self.buffer_index] = (self.scratch << (8 - self.scratch_index)).reverse_bits();
            self.buffer_index += 1;
        }

        let output_length = self.buffer_index;

        self.buffer_index = 0;
        self.scratch_index = 0;
        self.scratch = 0;

        let mut output_buffer = [0; MAX_BUFFER_SIZE];
        output_buffer.clone_from_slice(&self.buffer[0..MAX_BUFFER_SIZE]);

        (output_length, output_buffer)
    }
}