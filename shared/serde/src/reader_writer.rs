use crate::consts::MAX_BUFFER_SIZE;

// BitWrite

pub trait BitWrite {
    fn write_bit(&mut self, bit: bool);
    fn write_byte(&mut self, byte: u8);
    fn bit_count(&self) -> u16;
}

// BitCounter
pub struct BitCounter {
    count: u16,
}

impl BitCounter {
    pub fn new() -> Self {
        Self {
            count: 0,
        }
    }
}

impl BitWrite for BitCounter {
    fn write_bit(&mut self, _: bool) {
        self.count += 1;
    }

    fn write_byte(&mut self, _: u8) {
        self.count += 8;
    }

    fn bit_count(&self) -> u16 {
        self.count
    }
}

// BitWriter

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
}

impl BitWriter {
    pub fn flush(&mut self) -> (usize, [u8; MAX_BUFFER_SIZE]) {
        if self.scratch_index > 0 {
            self.buffer[self.buffer_index] =
                (self.scratch << (8 - self.scratch_index)).reverse_bits();
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

impl BitWrite for BitWriter {
    fn write_bit(&mut self, bit: bool) {
        self.scratch <<= 1;

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

    fn write_byte(&mut self, byte: u8) {
        let mut temp = byte;
        for _ in 0..8 {
            self.write_bit(temp & 1 != 0);
            temp >>= 1;
        }
    }

    fn bit_count(&self) -> u16 {
        ((self.buffer_index * 8) + (self.scratch_index as usize)) as u16
    }
}

// BitReader

pub struct BitReader<'b> {
    state: BitReaderState,
    buffer: &'b [u8],
}

impl<'b> BitReader<'b> {
    pub fn new(buffer: &'b [u8]) -> Self {
        Self {
            state: BitReaderState {
                scratch: 0,
                scratch_index: 0,
                buffer_index: 0,
            },
            buffer,
        }
    }

    pub fn to_owned(&self) -> OwnedBitReader {
        OwnedBitReader {
            state: self.state,
            buffer: self.buffer.into(),
        }
    }

    pub(crate) fn read_bit(&mut self) -> bool {
        if self.state.scratch_index == 0 {
            if self.state.buffer_index == self.buffer.len() {
                panic!("no more bytes to read");
            }

            self.state.scratch = self.buffer[self.state.buffer_index];

            self.state.buffer_index += 1;
            self.state.scratch_index += 8;
        }

        let value = self.state.scratch & 1;

        self.state.scratch >>= 1;

        self.state.scratch_index -= 1;

        value != 0
    }

    pub(crate) fn read_byte(&mut self) -> u8 {
        let mut output = 0;
        for _ in 0..7 {
            if self.read_bit() {
                output |= 128;
            }
            output >>= 1;
        }
        if self.read_bit() {
            output |= 128;
        }
        output
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
                scratch_index: 0,
                buffer_index: 0,
            },
            buffer: buffer.into(),
        }
    }

    pub fn borrow(&self) -> BitReader {
        BitReader {
            state: self.state,
            buffer: &self.buffer,
        }
    }
}

// BitReaderState
#[derive(Copy, Clone)]
struct BitReaderState {
    scratch: u8,
    scratch_index: u8,
    buffer_index: usize,
}

mod tests {

    #[test]
    fn read_write_1_bit() {
        use crate::reader_writer::{BitReader, BitWrite, BitWriter};

        let mut writer = BitWriter::new();

        writer.write_bit(true);

        let (buffer_length, buffer) = writer.flush();

        let mut reader = BitReader::new(&buffer[..buffer_length]);

        assert!(reader.read_bit());
    }

    #[test]
    fn read_write_3_bits() {
        use crate::reader_writer::{BitReader, BitWrite, BitWriter};

        let mut writer = BitWriter::new();

        writer.write_bit(false);
        writer.write_bit(true);
        writer.write_bit(true);

        let (buffer_length, buffer) = writer.flush();

        let mut reader = BitReader::new(&buffer[..buffer_length]);

        assert!(!reader.read_bit());
        assert!(reader.read_bit());
        assert!(reader.read_bit());
    }

    #[test]
    fn read_write_8_bits() {
        use crate::reader_writer::{BitReader, BitWrite, BitWriter};

        let mut writer = BitWriter::new();

        writer.write_bit(false);
        writer.write_bit(true);
        writer.write_bit(false);
        writer.write_bit(true);

        writer.write_bit(true);
        writer.write_bit(false);
        writer.write_bit(false);
        writer.write_bit(false);

        let (buffer_length, buffer) = writer.flush();

        let mut reader = BitReader::new(&buffer[..buffer_length]);

        assert!(!reader.read_bit());
        assert!(reader.read_bit());
        assert!(!reader.read_bit());
        assert!(reader.read_bit());

        assert!(reader.read_bit());
        assert!(!reader.read_bit());
        assert!(!reader.read_bit());
        assert!(!reader.read_bit());
    }

    #[test]
    fn read_write_13_bits() {
        use crate::reader_writer::{BitReader, BitWrite, BitWriter};

        let mut writer = BitWriter::new();

        writer.write_bit(false);
        writer.write_bit(true);
        writer.write_bit(false);
        writer.write_bit(true);

        writer.write_bit(true);
        writer.write_bit(false);
        writer.write_bit(false);
        writer.write_bit(false);

        writer.write_bit(true);
        writer.write_bit(false);
        writer.write_bit(true);
        writer.write_bit(true);

        writer.write_bit(true);

        let (buffer_length, buffer) = writer.flush();

        let mut reader = BitReader::new(&buffer[..buffer_length]);

        assert!(!reader.read_bit());
        assert!(reader.read_bit());
        assert!(!reader.read_bit());
        assert!(reader.read_bit());

        assert!(reader.read_bit());
        assert!(!reader.read_bit());
        assert!(!reader.read_bit());
        assert!(!reader.read_bit());

        assert!(reader.read_bit());
        assert!(!reader.read_bit());
        assert!(reader.read_bit());
        assert!(reader.read_bit());

        assert!(reader.read_bit());
    }

    #[test]
    fn read_write_16_bits() {
        use crate::reader_writer::{BitReader, BitWrite, BitWriter};

        let mut writer = BitWriter::new();

        writer.write_bit(false);
        writer.write_bit(true);
        writer.write_bit(false);
        writer.write_bit(true);

        writer.write_bit(true);
        writer.write_bit(false);
        writer.write_bit(false);
        writer.write_bit(false);

        writer.write_bit(true);
        writer.write_bit(false);
        writer.write_bit(true);
        writer.write_bit(true);

        writer.write_bit(true);
        writer.write_bit(false);
        writer.write_bit(true);
        writer.write_bit(true);

        let (buffer_length, buffer) = writer.flush();

        let mut reader = BitReader::new(&buffer[..buffer_length]);

        assert!(!reader.read_bit());
        assert!(reader.read_bit());
        assert!(!reader.read_bit());
        assert!(reader.read_bit());

        assert!(reader.read_bit());
        assert!(!reader.read_bit());
        assert!(!reader.read_bit());
        assert!(!reader.read_bit());

        assert!(reader.read_bit());
        assert!(!reader.read_bit());
        assert!(reader.read_bit());
        assert!(reader.read_bit());

        assert!(reader.read_bit());
        assert!(!reader.read_bit());
        assert!(reader.read_bit());
        assert!(reader.read_bit());
    }

    #[test]
    fn read_write_1_byte() {
        use crate::reader_writer::{BitReader, BitWrite, BitWriter};

        let mut writer = BitWriter::new();

        writer.write_byte(123);

        let (buffer_length, buffer) = writer.flush();

        let mut reader = BitReader::new(&buffer[..buffer_length]);

        assert_eq!(123, reader.read_byte());
    }

    #[test]
    fn read_write_5_bytes() {
        use crate::reader_writer::{BitReader, BitWrite, BitWriter};

        let mut writer = BitWriter::new();

        writer.write_byte(48);
        writer.write_byte(151);
        writer.write_byte(62);
        writer.write_byte(34);
        writer.write_byte(2);

        let (buffer_length, buffer) = writer.flush();

        let mut reader = BitReader::new(&buffer[..buffer_length]);

        assert_eq!(48, reader.read_byte());
        assert_eq!(151, reader.read_byte());
        assert_eq!(62, reader.read_byte());
        assert_eq!(34, reader.read_byte());
        assert_eq!(2, reader.read_byte());
    }
}
