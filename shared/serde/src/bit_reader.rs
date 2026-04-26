// BitReader
//
// Internals: bits are consumed from a u32 scratch register, refilled one byte
// at a time from the wire buffer. Bits are stored LSB-first, matching the
// writer's encoding, which lets the decoder accumulate values without any
// reverse_bits call. u32 scratch keeps all operations native on wasm32.

use crate::SerdeErr;

pub struct BitReader<'b> {
    state: BitReaderState,
    buffer: &'b [u8],
}

impl<'b> BitReader<'b> {
    pub fn new(buffer: &'b [u8]) -> Self {
        Self {
            state: BitReaderState {
                scratch: 0,
                scratch_bits: 0,
                buffer_index: 0,
            },
            buffer,
        }
    }

    pub fn bytes_len(&self) -> usize {
        self.buffer.len()
    }

    pub fn to_owned(&self) -> OwnedBitReader {
        OwnedBitReader {
            state: self.state,
            buffer: self.buffer.into(),
        }
    }

    #[inline(always)]
    pub fn read_bit(&mut self) -> Result<bool, SerdeErr> {
        if self.state.scratch_bits == 0 {
            if self.state.buffer_index == self.buffer.len() {
                return Err(SerdeErr);
            }
            self.state.scratch = self.buffer[self.state.buffer_index] as u32;
            self.state.buffer_index += 1;
            self.state.scratch_bits = 8;
        }
        let bit = self.state.scratch & 1 != 0;
        self.state.scratch >>= 1;
        self.state.scratch_bits -= 1;
        Ok(bit)
    }

    #[inline(always)]
    pub(crate) fn read_byte(&mut self) -> Result<u8, SerdeErr> {
        // Fast path: a full byte is already in scratch.
        if self.state.scratch_bits >= 8 {
            let byte = (self.state.scratch & 0xFF) as u8;
            self.state.scratch >>= 8;
            self.state.scratch_bits -= 8;
            return Ok(byte);
        }
        // General path: accumulate 8 bits LSB-first.
        let mut output = 0u8;
        for i in 0..8u8 {
            if self.read_bit()? {
                output |= 1 << i;
            }
        }
        Ok(output)
    }
}

// OwnedBitReader

pub struct OwnedBitReader {
    state: BitReaderState,
    buffer: Box<[u8]>,
}

impl OwnedBitReader {
    pub fn new(buffer: &[u8]) -> Self {
        Self {
            state: BitReaderState {
                scratch: 0,
                scratch_bits: 0,
                buffer_index: 0,
            },
            buffer: buffer.into(),
        }
    }

    pub fn borrow(&'_ self) -> BitReader<'_> {
        BitReader {
            state: self.state,
            buffer: &self.buffer,
        }
    }

    pub fn take_buffer(self) -> Box<[u8]> {
        self.buffer
    }
}

// BitReaderState
#[derive(Copy, Clone)]
struct BitReaderState {
    scratch: u32,
    scratch_bits: u32,
    buffer_index: usize,
}
