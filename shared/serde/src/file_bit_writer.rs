use crate::BitWrite;

// FileBitWriter
pub struct FileBitWriter {
    scratch: u8,
    scratch_index: u8,
    buffer: Vec<u8>,
}

impl FileBitWriter {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            scratch: 0,
            scratch_index: 0,
            buffer: Vec::new(),
        }
    }

    fn finalize(&mut self) {
        if self.scratch_index > 0 {
            let value = (self.scratch << (8 - self.scratch_index)).reverse_bits();
            self.buffer.push(value);
        }
    }

    pub fn to_bytes(mut self) -> Box<[u8]> {
        self.finalize();
        Box::from(self.buffer)
    }
}

impl BitWrite for FileBitWriter {
    fn write_bit(&mut self, bit: bool) {

        self.scratch <<= 1;

        if bit {
            self.scratch |= 1;
        }

        self.scratch_index += 1;

        if self.scratch_index >= 8 {
            let value = self.scratch.reverse_bits();
            self.buffer.push(value);
            self.scratch_index -= 8;
            self.scratch = 0;
        }
    }

    fn write_byte(&mut self, byte: u8) {
        let mut temp = byte;
        for _ in 0..8 {
            self.write_bit(temp & 1 != 0);
            temp >>= 1;
        }
    }

    fn write_bits(&mut self, _: u32) {
        panic!("This method should not be called for FileBitWriter!");
    }

    fn is_counter(&self) -> bool {
        panic!("This method should not be called for FileBitWriter!");
    }
}