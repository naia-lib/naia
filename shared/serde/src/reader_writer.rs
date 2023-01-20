use crate::{MTU_SIZE_BYTES, MTU_SIZE_BITS, SerdeErr, WriteOverflowError};

// BitWrite
pub trait BitWrite {
    fn write_bit(&mut self, bit: bool) -> Result<(), WriteOverflowError>;
    fn write_byte(&mut self, byte: u8) -> Result<(), WriteOverflowError>;
    fn bit_count(&self) -> usize;
}

// BitCounter
pub struct BitCounter {
    count: usize,
}

impl BitCounter {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self { count: 0 }
    }
}

impl BitWrite for BitCounter {
    fn write_bit(&mut self, _: bool) -> Result<(), WriteOverflowError> {
        self.count += 1;
        if self.count > MTU_SIZE_BITS {
            Err(WriteOverflowError)
        } else {
            Ok(())
        }
    }

    fn write_byte(&mut self, _: u8) -> Result<(), WriteOverflowError> {
        self.count += 8;
        if self.count > MTU_SIZE_BITS {
            Err(WriteOverflowError)
        } else {
            Ok(())
        }
    }

    fn bit_count(&self) -> usize {
        self.count
    }
}

// BitWriter

pub struct BitWriter {
    scratch: u8,
    scratch_index: u8,
    buffer: [u8; MTU_SIZE_BYTES as usize],
    buffer_index: usize,
    full: bool,
}

impl BitWriter {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            scratch: 0,
            scratch_index: 0,
            buffer: [0; MTU_SIZE_BYTES],
            buffer_index: 0,
            full: false,
        }
    }
}

impl BitWriter {
    pub fn flush(&mut self) -> (usize, [u8; MTU_SIZE_BYTES]) {
        if self.scratch_index > 0 {
            self.buffer[self.buffer_index] =
                (self.scratch << (8 - self.scratch_index)).reverse_bits();
            self.buffer_index += 1;
        }

        let output_length = self.buffer_index;

        self.buffer_index = 0;
        self.scratch_index = 0;
        self.scratch = 0;

        let mut output_buffer = [0; MTU_SIZE_BYTES];
        output_buffer.clone_from_slice(&self.buffer[0..MTU_SIZE_BYTES]);

        (output_length, output_buffer)
    }
}

impl BitWrite for BitWriter {
    fn write_bit(&mut self, bit: bool) -> Result<(), WriteOverflowError> {
        if self.full {
            return Err(WriteOverflowError);
        }

        self.scratch <<= 1;

        if bit {
            self.scratch |= 1;
        }

        self.scratch_index += 1;

        if self.scratch_index >= 8 {
            self.buffer[self.buffer_index] = self.scratch.reverse_bits();

            self.scratch_index -= 8;
            self.scratch = 0;

            self.buffer_index += 1;
            if self.buffer_index == MTU_SIZE_BYTES {
                self.full = true;
            }
        }

        Ok(())
    }

    fn write_byte(&mut self, byte: u8) -> Result<(), WriteOverflowError> {
        let mut temp = byte;
        for _ in 0..8 {
            let write_result = self.write_bit(temp & 1 != 0);
            if write_result.is_err() {
                return write_result;
            }
            temp >>= 1;
        }
        Ok(())
    }

    fn bit_count(&self) -> usize {
        (self.buffer_index * 8) + (self.scratch_index as usize)
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

    pub(crate) fn read_bit(&mut self) -> Result<bool, SerdeErr> {
        if self.state.scratch_index == 0 {
            if self.state.buffer_index == self.buffer.len() {
                return Err(SerdeErr);
            }

            self.state.scratch = self.buffer[self.state.buffer_index];

            self.state.buffer_index += 1;
            self.state.scratch_index += 8;
        }

        let value = self.state.scratch & 1;

        self.state.scratch >>= 1;

        self.state.scratch_index -= 1;

        Ok(value != 0)
    }

    pub(crate) fn read_byte(&mut self) -> Result<u8, SerdeErr> {
        let mut output = 0;
        for _ in 0..7 {
            if self.read_bit()? {
                output |= 128;
            }
            output >>= 1;
        }
        if self.read_bit()? {
            output |= 128;
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

        writer.write_bit(true).unwrap();

        let (buffer_length, buffer) = writer.flush();

        let mut reader = BitReader::new(&buffer[..buffer_length]);

        assert!(reader.read_bit().unwrap());
    }

    #[test]
    fn read_write_3_bits() {
        use crate::reader_writer::{BitReader, BitWrite, BitWriter};

        let mut writer = BitWriter::new();

        writer.write_bit(false).unwrap();
        writer.write_bit(true).unwrap();
        writer.write_bit(true).unwrap();

        let (buffer_length, buffer) = writer.flush();

        let mut reader = BitReader::new(&buffer[..buffer_length]);

        assert!(!reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());
    }

    #[test]
    fn read_write_8_bits() {
        use crate::reader_writer::{BitReader, BitWrite, BitWriter};

        let mut writer = BitWriter::new();

        writer.write_bit(false).unwrap();
        writer.write_bit(true).unwrap();
        writer.write_bit(false).unwrap();
        writer.write_bit(true).unwrap();

        writer.write_bit(true).unwrap();
        writer.write_bit(false).unwrap();
        writer.write_bit(false).unwrap();
        writer.write_bit(false).unwrap();

        let (buffer_length, buffer) = writer.flush();

        let mut reader = BitReader::new(&buffer[..buffer_length]);

        assert!(!reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());

        assert!(reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
    }

    #[test]
    fn read_write_13_bits() {
        use crate::reader_writer::{BitReader, BitWrite, BitWriter};

        let mut writer = BitWriter::new();

        writer.write_bit(false).unwrap();
        writer.write_bit(true).unwrap();
        writer.write_bit(false).unwrap();
        writer.write_bit(true).unwrap();

        writer.write_bit(true).unwrap();
        writer.write_bit(false).unwrap();
        writer.write_bit(false).unwrap();
        writer.write_bit(false).unwrap();

        writer.write_bit(true).unwrap();
        writer.write_bit(false).unwrap();
        writer.write_bit(true).unwrap();
        writer.write_bit(true).unwrap();

        writer.write_bit(true).unwrap();

        let (buffer_length, buffer) = writer.flush();

        let mut reader = BitReader::new(&buffer[..buffer_length]);

        assert!(!reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());

        assert!(reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());

        assert!(reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());

        assert!(reader.read_bit().unwrap());
    }

    #[test]
    fn read_write_16_bits() {
        use crate::reader_writer::{BitReader, BitWrite, BitWriter};

        let mut writer = BitWriter::new();

        writer.write_bit(false).unwrap();
        writer.write_bit(true).unwrap();
        writer.write_bit(false).unwrap();
        writer.write_bit(true).unwrap();

        writer.write_bit(true).unwrap();
        writer.write_bit(false).unwrap();
        writer.write_bit(false).unwrap();
        writer.write_bit(false).unwrap();

        writer.write_bit(true).unwrap();
        writer.write_bit(false).unwrap();
        writer.write_bit(true).unwrap();
        writer.write_bit(true).unwrap();

        writer.write_bit(true).unwrap();
        writer.write_bit(false).unwrap();
        writer.write_bit(true).unwrap();
        writer.write_bit(true).unwrap();

        let (buffer_length, buffer) = writer.flush();

        let mut reader = BitReader::new(&buffer[..buffer_length]);

        assert!(!reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());

        assert!(reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());

        assert!(reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());

        assert!(reader.read_bit().unwrap());
        assert!(!reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());
    }

    #[test]
    fn read_write_1_byte() {
        use crate::reader_writer::{BitReader, BitWrite, BitWriter};

        let mut writer = BitWriter::new();

        writer.write_byte(123).unwrap();

        let (buffer_length, buffer) = writer.flush();

        let mut reader = BitReader::new(&buffer[..buffer_length]);

        assert_eq!(123, reader.read_byte().unwrap());
    }

    #[test]
    fn read_write_5_bytes() {
        use crate::reader_writer::{BitReader, BitWrite, BitWriter};

        let mut writer = BitWriter::new();

        writer.write_byte(48).unwrap();
        writer.write_byte(151).unwrap();
        writer.write_byte(62).unwrap();
        writer.write_byte(34).unwrap();
        writer.write_byte(2).unwrap();

        let (buffer_length, buffer) = writer.flush();

        let mut reader = BitReader::new(&buffer[..buffer_length]);

        assert_eq!(48, reader.read_byte().unwrap());
        assert_eq!(151, reader.read_byte().unwrap());
        assert_eq!(62, reader.read_byte().unwrap());
        assert_eq!(34, reader.read_byte().unwrap());
        assert_eq!(2, reader.read_byte().unwrap());
    }
}
