use crate::consts::MAX_BUFFER_SIZE;
use crate::error::DeErr;
use crate::traits::De;

pub struct BitReader {
    scratch: u8,
    scratch_index: u8,
    buffer: [u8; MAX_BUFFER_SIZE],
    buffer_index: usize,
    buffer_length: usize,
}

impl BitReader {
    pub fn new(buffer_length: usize, buffer: [u8; MAX_BUFFER_SIZE]) -> Self {
        Self {
            scratch: 0,
            scratch_index: 0,
            buffer,
            buffer_index: 0,
            buffer_length
        }
    }

    pub fn read<T: De>(&mut self) -> Result<T, DeErr> {
        T::de(self)
    }

    pub fn read_bit(&mut self) -> bool {

        if self.scratch_index <= 0 {

            if self.buffer_index == self.buffer_length {
                panic!("no more bytes to read");
            }

            self.scratch = self.buffer[self.buffer_index];

            self.buffer_index += 1;
            self.scratch_index += 8;
        }

        let value = self.scratch & 1;

        self.scratch = self.scratch >> 1;

        self.scratch_index -= 1;

        value != 0
    }
}