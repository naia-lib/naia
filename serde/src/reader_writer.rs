use crate::{consts::MAX_BUFFER_SIZE, error::SerdeErr, serde::Serde};

// Writer

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

    pub fn write<T: Serde>(&mut self, target: &T) {
        target.ser(self);
    }

    pub(crate) fn write_bit(&mut self, bit: bool) {
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

    pub(crate) fn write_byte(&mut self, byte: u8) {
        let mut temp = byte;
        for _ in 0..8 {
            self.write_bit(temp & 1 != 0);
            temp = temp >> 1;
        }
    }

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

// Reader

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
            buffer_length,
        }
    }

    pub fn read<T: Serde>(&mut self) -> Result<T, SerdeErr> {
        T::de(self)
    }

    pub(crate) fn read_bit(&mut self) -> bool {
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

    pub(crate) fn read_byte(&mut self) -> u8 {
        let mut output = 0;
        for _ in 0..7 {
            if self.read_bit() {
                output = output | 128;
            }
            output = output >> 1;
        }
        if self.read_bit() {
            output = output | 128;
        }
        output
    }
}

mod tests {

    #[test]
    fn read_write_1_bit() {
        use crate::{reader_writer::{BitReader, BitWriter}};

        let mut writer = BitWriter::new();

        writer.write_bit(true);

        let (buffer_length, buffer) = writer.flush();

        let mut reader = BitReader::new(buffer_length, buffer);

        assert_eq!(true, reader.read_bit());
    }

    #[test]
    fn read_write_3_bits() {
        use crate::{reader_writer::{BitReader, BitWriter}};

        let mut writer = BitWriter::new();

        writer.write_bit(false);
        writer.write_bit(true);
        writer.write_bit(true);

        let (buffer_length, buffer) = writer.flush();

        let mut reader = BitReader::new(buffer_length, buffer);

        assert_eq!(false, reader.read_bit());
        assert_eq!(true, reader.read_bit());
        assert_eq!(true, reader.read_bit());
    }

    #[test]
    fn read_write_8_bits() {
        use crate::{reader_writer::{BitReader, BitWriter}};

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

        let mut reader = BitReader::new(buffer_length, buffer);

        assert_eq!(false, reader.read_bit());
        assert_eq!(true, reader.read_bit());
        assert_eq!(false, reader.read_bit());
        assert_eq!(true, reader.read_bit());

        assert_eq!(true, reader.read_bit());
        assert_eq!(false, reader.read_bit());
        assert_eq!(false, reader.read_bit());
        assert_eq!(false, reader.read_bit());
    }

    #[test]
    fn read_write_13_bits() {
        use crate::{reader_writer::{BitReader, BitWriter}};

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

        let mut reader = BitReader::new(buffer_length, buffer);

        assert_eq!(false, reader.read_bit());
        assert_eq!(true, reader.read_bit());
        assert_eq!(false, reader.read_bit());
        assert_eq!(true, reader.read_bit());

        assert_eq!(true, reader.read_bit());
        assert_eq!(false, reader.read_bit());
        assert_eq!(false, reader.read_bit());
        assert_eq!(false, reader.read_bit());

        assert_eq!(true, reader.read_bit());
        assert_eq!(false, reader.read_bit());
        assert_eq!(true, reader.read_bit());
        assert_eq!(true, reader.read_bit());

        assert_eq!(true, reader.read_bit());
    }

    #[test]
    fn read_write_16_bits() {
        use crate::{reader_writer::{BitReader, BitWriter}};

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

        let mut reader = BitReader::new(buffer_length, buffer);

        assert_eq!(false, reader.read_bit());
        assert_eq!(true, reader.read_bit());
        assert_eq!(false, reader.read_bit());
        assert_eq!(true, reader.read_bit());

        assert_eq!(true, reader.read_bit());
        assert_eq!(false, reader.read_bit());
        assert_eq!(false, reader.read_bit());
        assert_eq!(false, reader.read_bit());

        assert_eq!(true, reader.read_bit());
        assert_eq!(false, reader.read_bit());
        assert_eq!(true, reader.read_bit());
        assert_eq!(true, reader.read_bit());

        assert_eq!(true, reader.read_bit());
        assert_eq!(false, reader.read_bit());
        assert_eq!(true, reader.read_bit());
        assert_eq!(true, reader.read_bit());
    }

    #[test]
    fn read_write_1_byte() {
        use crate::{reader_writer::{BitReader, BitWriter}};

        let mut writer = BitWriter::new();

        writer.write_byte(123);

        let (buffer_length, buffer) = writer.flush();

        let mut reader = BitReader::new(buffer_length, buffer);

        assert_eq!(123, reader.read_byte());
    }

    #[test]
    fn read_write_5_bytes() {
        use crate::{reader_writer::{BitReader, BitWriter}};

        let mut writer = BitWriter::new();

        writer.write_byte(48);
        writer.write_byte(151);
        writer.write_byte(62);
        writer.write_byte(34);
        writer.write_byte(2);

        let (buffer_length, buffer) = writer.flush();

        let mut reader = BitReader::new(buffer_length, buffer);

        assert_eq!(48, reader.read_byte());
        assert_eq!(151, reader.read_byte());
        assert_eq!(62, reader.read_byte());
        assert_eq!(34, reader.read_byte());
        assert_eq!(2, reader.read_byte());
    }
}