use crate::BitWrite;

// FileBitWriter — heap-backed writer for files/snapshots, no MTU cap.
// Uses the same u32-scratch word-aligned approach as BitWriter.
pub struct FileBitWriter {
    scratch: u32,
    scratch_bits: u32,
    buffer: Vec<u8>,
}

impl FileBitWriter {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            scratch: 0,
            scratch_bits: 0,
            buffer: Vec::new(),
        }
    }

    fn flush_word(&mut self) {
        self.buffer.extend_from_slice(&self.scratch.to_le_bytes());
        self.scratch = 0;
        self.scratch_bits = 0;
    }

    fn finalize(&mut self) {
        if self.scratch_bits > 0 {
            let remaining_bytes = (self.scratch_bits as usize + 7) / 8;
            let word = self.scratch.to_le_bytes();
            self.buffer.extend_from_slice(&word[..remaining_bytes]);
        }
    }

    pub fn to_bytes(mut self) -> Box<[u8]> {
        self.finalize();
        Box::from(self.buffer)
    }

    pub fn to_vec(mut self) -> Vec<u8> {
        self.finalize();
        self.buffer
    }
}

impl BitWrite for FileBitWriter {
    #[inline(always)]
    fn write_bit(&mut self, bit: bool) {
        self.scratch |= (bit as u32) << self.scratch_bits;
        self.scratch_bits += 1;
        if self.scratch_bits == 32 {
            self.flush_word();
        }
    }

    #[inline(always)]
    fn write_byte(&mut self, byte: u8) {
        let available = 32 - self.scratch_bits;
        if available >= 8 {
            self.scratch |= (byte as u32) << self.scratch_bits;
            self.scratch_bits += 8;
            if self.scratch_bits == 32 {
                self.flush_word();
            }
        } else {
            let lo = (byte as u32) & ((1 << available) - 1);
            self.scratch |= lo << self.scratch_bits;
            self.flush_word();
            self.scratch = (byte as u32) >> available;
            self.scratch_bits = 8 - available;
        }
    }

    fn count_bits(&mut self, _: u32) {
        panic!("This method should not be called for FileBitWriter!");
    }

    fn is_counter(&self) -> bool {
        panic!("This method should not be called for FileBitWriter!");
    }
}
