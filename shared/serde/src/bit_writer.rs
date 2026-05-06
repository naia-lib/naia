use crate::{
    constants::{MTU_SIZE_BITS, MTU_SIZE_BYTES},
    BitCounter, OutgoingPacket, OwnedBitReader,
};

// BitWrite
pub trait BitWrite {
    fn write_bit(&mut self, bit: bool);
    fn write_byte(&mut self, byte: u8);

    fn is_counter(&self) -> bool;
    fn count_bits(&mut self, bits: u32);
}

// BitWriter
//
// Internals: bits accumulate LSB-first into a u32 scratch register and flush
// as a little-endian u32 word every 32 bits. Using u32 (not u64) keeps all
// operations native on wasm32 targets where 64-bit arithmetic is emulated.
// The approach eliminates the per-byte reverse_bits call of the old u8 design.
// (Inspired by Gaffer on Games, "Reading and Writing Packets", 2015.)
pub struct BitWriter {
    scratch: u32,
    scratch_bits: u32,
    buffer: [u8; MTU_SIZE_BYTES],
    byte_count: usize,
    current_bits: u32,
    max_bits: u32,
}

impl BitWriter {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            scratch: 0,
            scratch_bits: 0,
            buffer: [0; MTU_SIZE_BYTES],
            byte_count: 0,
            current_bits: 0,
            max_bits: MTU_SIZE_BITS,
        }
    }

    pub fn with_capacity(bit_capacity: u32) -> Self {
        Self {
            scratch: 0,
            scratch_bits: 0,
            buffer: [0; MTU_SIZE_BYTES],
            byte_count: 0,
            current_bits: 0,
            max_bits: bit_capacity,
        }
    }

    pub fn with_max_capacity() -> Self {
        Self::with_capacity(u32::MAX)
    }

    fn flush_word(&mut self) {
        self.buffer[self.byte_count..self.byte_count + 4]
            .copy_from_slice(&self.scratch.to_le_bytes());
        self.byte_count += 4;
        self.scratch = 0;
        self.scratch_bits = 0;
    }

    fn finalize(&mut self) {
        if self.scratch_bits > 0 {
            let remaining_bytes = (self.scratch_bits as usize).div_ceil(8);
            let word = self.scratch.to_le_bytes();
            self.buffer[self.byte_count..self.byte_count + remaining_bytes]
                .copy_from_slice(&word[..remaining_bytes]);
            self.byte_count += remaining_bytes;
        }
        self.max_bits = 0;
    }

    pub fn to_packet(mut self) -> OutgoingPacket {
        self.finalize();
        OutgoingPacket::new(self.byte_count, self.buffer)
    }

    pub fn to_owned_reader(mut self) -> OwnedBitReader {
        self.finalize();
        OwnedBitReader::new(&self.buffer[0..self.byte_count])
    }

    pub fn to_bytes(mut self) -> Box<[u8]> {
        self.finalize();
        Box::from(&self.buffer[0..self.byte_count])
    }

    pub fn counter(&self) -> BitCounter {
        BitCounter::new(self.current_bits, self.current_bits, self.max_bits)
    }

    pub fn reserve_bits(&mut self, bits: u32) {
        self.max_bits -= bits;
    }

    pub fn release_bits(&mut self, bits: u32) {
        self.max_bits += bits;
    }

    pub fn bits_free(&self) -> u32 {
        self.max_bits - self.current_bits
    }
}

impl BitWrite for BitWriter {
    #[inline(always)]
    fn write_bit(&mut self, bit: bool) {
        if self.current_bits >= self.max_bits {
            panic!("Write overflow!");
        }
        self.scratch |= (bit as u32) << self.scratch_bits;
        self.scratch_bits += 1;
        self.current_bits += 1;
        if self.scratch_bits == 32 {
            self.flush_word();
        }
    }

    #[inline(always)]
    fn write_byte(&mut self, byte: u8) {
        if self.current_bits + 8 > self.max_bits {
            panic!("Write overflow!");
        }
        self.current_bits += 8;
        let available = 32 - self.scratch_bits;
        if available >= 8 {
            self.scratch |= (byte as u32) << self.scratch_bits;
            self.scratch_bits += 8;
            if self.scratch_bits == 32 {
                self.flush_word();
            }
        } else {
            // byte spans a 32-bit word boundary
            let lo = (byte as u32) & ((1 << available) - 1);
            self.scratch |= lo << self.scratch_bits;
            self.flush_word();
            self.scratch = (byte as u32) >> available;
            self.scratch_bits = 8 - available;
        }
    }

    fn count_bits(&mut self, _: u32) {
        panic!("This method should not be called for BitWriter!");
    }

    fn is_counter(&self) -> bool {
        false
    }
}

mod tests {

    // ─── word-boundary regression tests (targets for word-aligned optimization) ─

    #[test]
    fn read_write_33_bits() {
        // 33 bits spans the 32-bit word boundary in the new implementation.
        use crate::{bit_reader::BitReader, bit_writer::{BitWrite, BitWriter}};
        let mut writer = BitWriter::with_max_capacity();
        // write 33 known bits: alternating pattern
        for i in 0..33usize {
            writer.write_bit(i % 3 == 0);
        }
        let buffer = writer.to_bytes();
        let mut reader = BitReader::new(&buffer);
        for i in 0..33usize {
            assert_eq!(reader.read_bit().unwrap(), i % 3 == 0, "bit {i} mismatch");
        }
    }

    #[test]
    fn read_write_64_bits_exact() {
        // exactly 2 words — tests two full-word flushes
        use crate::{bit_reader::BitReader, bit_writer::{BitWrite, BitWriter}};
        let mut writer = BitWriter::with_max_capacity();
        for i in 0..64usize {
            writer.write_bit(i % 5 < 2);
        }
        let buffer = writer.to_bytes();
        let mut reader = BitReader::new(&buffer);
        for i in 0..64usize {
            assert_eq!(reader.read_bit().unwrap(), i % 5 < 2, "bit {i} mismatch");
        }
    }

    #[test]
    fn read_write_5_bytes_via_write_byte_then_read_bit() {
        // mix write_byte (8 aligned) with read_bit to verify no endian confusion
        use crate::{bit_reader::BitReader, bit_writer::{BitWrite, BitWriter}};
        let data: &[u8] = &[0b10110001, 0b01001110, 0b11010101, 0b00110011, 0b11111010];
        let mut writer = BitWriter::with_max_capacity();
        for &b in data {
            writer.write_byte(b);
        }
        let buffer = writer.to_bytes();
        let mut reader = BitReader::new(&buffer);
        for &b in data {
            for bit in 0..8usize {
                let expected = (b >> bit) & 1 != 0;
                assert_eq!(reader.read_bit().unwrap(), expected, "byte {b:#010b} bit {bit}");
            }
        }
    }

    // ─── existing bit/byte round-trip tests ────────────────────────────────────

    #[test]
    fn read_write_1_bit() {
        use crate::{
            bit_reader::BitReader,
            bit_writer::{BitWrite, BitWriter},
        };

        let mut writer = BitWriter::new();

        writer.write_bit(true);

        let buffer = writer.to_bytes();

        let mut reader = BitReader::new(&buffer);

        assert!(reader.read_bit().unwrap());
    }

    #[test]
    fn read_write_3_bits() {
        use crate::{
            bit_reader::BitReader,
            bit_writer::{BitWrite, BitWriter},
        };

        let mut writer = BitWriter::new();

        writer.write_bit(false);
        writer.write_bit(true);
        writer.write_bit(true);

        let buffer = writer.to_bytes();

        let mut reader = BitReader::new(&buffer);

        assert!(!reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());
        assert!(reader.read_bit().unwrap());
    }

    #[test]
    fn read_write_8_bits() {
        use crate::{
            bit_reader::BitReader,
            bit_writer::{BitWrite, BitWriter},
        };

        let mut writer = BitWriter::new();

        writer.write_bit(false);
        writer.write_bit(true);
        writer.write_bit(false);
        writer.write_bit(true);

        writer.write_bit(true);
        writer.write_bit(false);
        writer.write_bit(false);
        writer.write_bit(false);

        let buffer = writer.to_bytes();

        let mut reader = BitReader::new(&buffer);

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
        use crate::{
            bit_reader::BitReader,
            bit_writer::{BitWrite, BitWriter},
        };

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

        let buffer = writer.to_bytes();

        let mut reader = BitReader::new(&buffer);

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
        use crate::{
            bit_reader::BitReader,
            bit_writer::{BitWrite, BitWriter},
        };

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

        let buffer = writer.to_bytes();

        let mut reader = BitReader::new(&buffer);

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
        use crate::{
            bit_reader::BitReader,
            bit_writer::{BitWrite, BitWriter},
        };

        let mut writer = BitWriter::new();

        writer.write_byte(123);

        let buffer = writer.to_bytes();

        let mut reader = BitReader::new(&buffer);

        assert_eq!(123, reader.read_byte().unwrap());
    }

    #[test]
    fn read_write_5_bytes() {
        use crate::{
            bit_reader::BitReader,
            bit_writer::{BitWrite, BitWriter},
        };

        let mut writer = BitWriter::new();

        writer.write_byte(48);
        writer.write_byte(151);
        writer.write_byte(62);
        writer.write_byte(34);
        writer.write_byte(2);

        let buffer = writer.to_bytes();

        let mut reader = BitReader::new(&buffer);

        assert_eq!(48, reader.read_byte().unwrap());
        assert_eq!(151, reader.read_byte().unwrap());
        assert_eq!(62, reader.read_byte().unwrap());
        assert_eq!(34, reader.read_byte().unwrap());
        assert_eq!(2, reader.read_byte().unwrap());
    }
}
