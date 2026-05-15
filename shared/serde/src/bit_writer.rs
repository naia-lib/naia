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

    /// Total bits written so far (flushed words + scratch register).
    pub fn bits_written(&self) -> u32 {
        self.current_bits
    }

    /// Slice of fully-flushed bytes (complete 32-bit words only).
    /// Does NOT include bits still in the scratch register.
    pub fn bytes_written_slice(&self) -> &[u8] {
        &self.buffer[..self.byte_count]
    }

    /// Returns `(scratch_value, scratch_bit_count)` — bits not yet flushed to buffer.
    /// `scratch_bit_count` is in [0, 31]. `scratch_value` holds that many valid LSB-first bits.
    pub fn scratch_bits_pending(&self) -> (u32, u32) {
        (self.scratch, self.scratch_bits)
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

/// Pre-serialized component body. Inline array, zero heap allocation.
/// 64 bytes = 512 bits. All registered components must fit within this limit
/// (enforced at ComponentKinds::add_component time via Replicate::max_bit_length()).
#[derive(Copy, Clone)]
pub struct CachedComponentUpdate {
    pub bytes: [u8; 64],
    pub bit_count: u32,
}

impl CachedComponentUpdate {
    /// Captures a BitWriter's current content into a CachedComponentUpdate.
    /// Must be called before finalize(). Returns None if total bit_count > 512.
    pub fn capture(writer: &BitWriter) -> Option<Self> {
        let bit_count = writer.bits_written();
        if bit_count > 512 {
            return None;
        }

        let flushed = writer.bytes_written_slice();
        let (scratch, scratch_bits) = writer.scratch_bits_pending();

        let mut bytes = [0u8; 64];
        bytes[..flushed.len()].copy_from_slice(flushed);

        if scratch_bits > 0 {
            let scratch_bytes = scratch.to_le_bytes();
            let n = (scratch_bits as usize).div_ceil(8);
            bytes[flushed.len()..flushed.len() + n].copy_from_slice(&scratch_bytes[..n]);
        }

        Some(Self { bytes, bit_count })
    }
}

/// Per-tick alignment counters for `append_cached_update`.
/// Enabled via `bench_instrumentation` feature.
#[cfg(feature = "bench_instrumentation")]
pub mod bench_serde_counters {
    use std::sync::atomic::{AtomicU64, Ordering};
    /// Calls where writer scratch_bits == 0 at entry (byte-aligned; memcpy path would apply).
    pub static N_APPEND_ALIGNED: AtomicU64 = AtomicU64::new(0);
    /// Calls where writer scratch_bits != 0 at entry (bit-unaligned; must bit-shift).
    pub static N_APPEND_UNALIGNED: AtomicU64 = AtomicU64::new(0);

    pub fn reset() {
        N_APPEND_ALIGNED.store(0, Ordering::Relaxed);
        N_APPEND_UNALIGNED.store(0, Ordering::Relaxed);
    }
    /// Returns (aligned_count, unaligned_count) since last reset.
    pub fn snapshot_alignment() -> (u64, u64) {
        (
            N_APPEND_ALIGNED.load(Ordering::Relaxed),
            N_APPEND_UNALIGNED.load(Ordering::Relaxed),
        )
    }
}

impl BitWriter {
    /// Appends all bits from a CachedComponentUpdate at the current write position.
    /// Bit-accurate at any alignment; produces bit-identical output to re-serializing the component.
    pub fn append_cached_update(&mut self, update: &CachedComponentUpdate) {
        if update.bit_count == 0 {
            return;
        }
        #[cfg(feature = "bench_instrumentation")]
        if self.scratch_bits == 0 {
            bench_serde_counters::N_APPEND_ALIGNED
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        } else {
            bench_serde_counters::N_APPEND_UNALIGNED
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let full_bytes = (update.bit_count / 8) as usize;
        let trailing = update.bit_count % 8;
        for &byte in &update.bytes[..full_bytes] {
            self.write_byte(byte);
        }
        if trailing > 0 {
            let last = update.bytes[full_bytes];
            for bit in 0..trailing {
                self.write_bit((last >> bit) & 1 != 0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BitWrite, BitWriter, CachedComponentUpdate};

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

    // ─── CachedComponentUpdate capture + append tests ─────────────────────────

    fn write_n_known_bits(writer: &mut BitWriter, n: u32) {
        for i in 0..n {
            writer.write_bit(i % 3 == 0);
        }
    }

    fn read_n_known_bits(reader: &mut crate::bit_reader::BitReader, n: u32) {
        for i in 0..n {
            let expected = i % 3 == 0;
            assert_eq!(
                reader.read_bit().unwrap(), expected,
                "bit {i} mismatch"
            );
        }
    }

    /// append_cached_update at ALL destination alignments 0-63
    #[test]
    fn cached_update_round_trips_at_all_alignments() {
        use crate::bit_reader::BitReader;
        const DATA_BITS: u32 = 37;
        // Build the source update
        let mut src = BitWriter::with_max_capacity();
        write_n_known_bits(&mut src, DATA_BITS);
        let cached = CachedComponentUpdate::capture(&src).unwrap();
        assert_eq!(cached.bit_count, DATA_BITS);

        for align in 0u32..64 {
            let mut dst = BitWriter::with_max_capacity();
            // Write `align` preamble bits (alternating)
            for i in 0..align {
                dst.write_bit(i % 2 == 0);
            }
            dst.append_cached_update(&cached);
            let buf = dst.to_bytes();
            let mut reader = BitReader::new(&buf);
            // Read preamble
            for i in 0..align {
                assert_eq!(reader.read_bit().unwrap(), i % 2 == 0,
                    "align={align} preamble bit {i}");
            }
            // Read cached data
            read_n_known_bits(&mut reader, DATA_BITS);
        }
    }

    /// capture with pending scratch bits (7 bits — stays in scratch register)
    #[test]
    fn capture_with_pending_scratch_bits() {
        use crate::bit_reader::BitReader;
        let mut src = BitWriter::with_max_capacity();
        write_n_known_bits(&mut src, 7);
        let cached = CachedComponentUpdate::capture(&src).unwrap();
        assert_eq!(cached.bit_count, 7);

        let mut dst = BitWriter::with_max_capacity();
        dst.append_cached_update(&cached);
        let buf = dst.to_bytes();
        let mut reader = BitReader::new(&buf);
        read_n_known_bits(&mut reader, 7);
    }

    /// capture across word boundary (33 bits spans 32-bit flush)
    #[test]
    fn capture_across_word_boundary() {
        use crate::bit_reader::BitReader;
        let mut src = BitWriter::with_max_capacity();
        write_n_known_bits(&mut src, 33);
        let cached = CachedComponentUpdate::capture(&src).unwrap();
        assert_eq!(cached.bit_count, 33);

        // Append at a non-zero alignment (17 bits)
        let mut dst = BitWriter::with_max_capacity();
        write_n_known_bits(&mut dst, 17);
        dst.append_cached_update(&cached);
        let buf = dst.to_bytes();
        let mut reader = BitReader::new(&buf);
        read_n_known_bits(&mut reader, 17);
        read_n_known_bits(&mut reader, 33);
    }

    /// 512-bit capture succeeds; 513-bit returns None
    #[test]
    fn capture_512_succeeds_513_fails() {
        let mut src512 = BitWriter::with_max_capacity();
        write_n_known_bits(&mut src512, 512);
        assert!(CachedComponentUpdate::capture(&src512).is_some());

        let mut src513 = BitWriter::with_max_capacity();
        write_n_known_bits(&mut src513, 513);
        assert!(CachedComponentUpdate::capture(&src513).is_none());
    }

    // ─── BitCounter::count_bits behavior test ─────────────────────────────────

    #[test]
    fn bit_counter_count_bits_accumulates() {
        use crate::bit_counter::BitCounter;
        let mut counter = BitCounter::new(0, 0, 1000);
        counter.count_bits(100);
        counter.count_bits(200);
        assert!(!counter.overflowed());
        assert_eq!(counter.bits_needed(), 300);
        counter.count_bits(701);
        assert!(counter.overflowed());
    }
}
