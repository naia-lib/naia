//! Variable-width lock-free atomic bitset.
//!
//! Generalizes the old single-`AtomicU64` pattern that capped at 64
//! bits. Bits are stored as `Box<[AtomicU64]>` of length
//! `ceil(bit_capacity / 64)`. Hot path operations (`set_bit`, `clear`,
//! `or_with`) use per-word atomic intrinsics — no locking, no growth
//! beyond the capacity set at construction.
//!
//! Used by:
//! - [`crate::AtomicDiffMask`] — per-property dirty bits per
//!   `MutReceiver`. Eliminates the historical "≤8 bytes / ≤64
//!   properties per component" limit.
//! - [`crate::DirtyQueue`] (flat-strided, see `mut_channel.rs`) —
//!   per-entity per-component-kind dirty bits. Eliminates the
//!   historical "≤64 component kinds" limit.
//!
//! ## "Was clear" semantics under multi-word
//!
//! For single-word bitsets, `set_bit` returning `was_clear == true`
//! means "this bit was the first dirty bit anywhere in the bitset"
//! — used to fire `notify_dirty` exactly once per clean→dirty
//! transition. Multi-word generalizes this to: the word we just
//! touched was zero before our `fetch_or` AND every other word in the
//! bitset is currently zero (relaxed load). Race tolerance: two
//! threads concurrently setting bits in different words may both see
//! "was clear" and both notify. The downstream consumer (`DirtyQueue`)
//! dedupes via its `indices` Vec + drain swap-zero, so duplicate
//! notifies are harmless. The contract is "at-least-once notify per
//! clean→dirty, with rare duplicates under race" — strictly weaker
//! than "exactly once" but sufficient for the existing call sites.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::DiffMask;

/// Number of `AtomicU64` words needed to hold `bit_capacity` bits.
#[inline]
fn words_for_bits(bit_capacity: usize) -> usize {
    bit_capacity.div_ceil(64).max(1)
}

/// Variable-width lock-free atomic bitset.
pub struct AtomicBitSet {
    /// Words of the bitset. Length = `ceil(bit_capacity / 64).max(1)`.
    /// Allocated once at construction; never resized.
    words: Box<[AtomicU64]>,
    /// Logical bit width. Stored separately from `words.len() * 64`
    /// because the wire format encodes a `byte_number` derived from
    /// this (e.g. the `DiffMask` snapshot).
    bit_capacity: usize,
}

impl AtomicBitSet {
    /// Create a new bitset with capacity for at least `bit_capacity`
    /// bits. Always allocates at least one word.
    pub fn new(bit_capacity: usize) -> Self {
        let n_words = words_for_bits(bit_capacity);
        let words: Box<[AtomicU64]> = (0..n_words).map(|_| AtomicU64::new(0)).collect();
        Self {
            words,
            bit_capacity,
        }
    }

    /// Number of bits the set can hold.
    #[inline]
    #[allow(dead_code)] // Symmetry with byte_number; used in tests + future callers.
    pub fn bit_capacity(&self) -> usize {
        self.bit_capacity
    }

    /// Number of bytes needed to encode the bit_capacity (rounded up
    /// to whole bytes — matches `DiffMask::byte_number()`).
    #[inline]
    pub fn byte_number(&self) -> usize {
        self.bit_capacity.div_ceil(8)
    }

    /// Set the bit at `index`. Returns `true` iff the entire bitset
    /// was clear before this call (modulo race-tolerance — see the
    /// module-level "Was clear semantics" docs).
    ///
    /// Out-of-range `index` is silently ignored (returns `false`).
    /// Panicking on OOB would create a hot-path crash surface for
    /// stale dirty indices across schema evolution.
    #[inline]
    pub fn set_bit(&self, index: u32) -> bool {
        let word_idx = (index / 64) as usize;
        let bit_in_word = index % 64;
        let Some(word) = self.words.get(word_idx) else {
            return false;
        };
        let prev = word.fetch_or(1u64 << bit_in_word, Ordering::Relaxed);
        if prev != 0 {
            return false;
        }
        // We zeroed out this word but only just now set a bit. To
        // report "whole bitset was clear" we need every OTHER word to
        // also be zero at this moment. Race-tolerant relaxed loads.
        if self.words.len() == 1 {
            return true;
        }
        for (i, w) in self.words.iter().enumerate() {
            if i == word_idx {
                continue;
            }
            if w.load(Ordering::Relaxed) != 0 {
                return false;
            }
        }
        true
    }

    /// Clear all bits. Returns `true` iff the bitset had any bit set
    /// (any word non-zero) before the clear.
    pub fn clear(&self) -> bool {
        let mut was_dirty = false;
        for word in self.words.iter() {
            let prev = word.swap(0, Ordering::Relaxed);
            if prev != 0 {
                was_dirty = true;
            }
        }
        was_dirty
    }

    /// True iff every word is zero.
    pub fn is_clear(&self) -> bool {
        self.words
            .iter()
            .all(|w| w.load(Ordering::Relaxed) == 0)
    }

    /// OR-merge another `DiffMask` (variable-length byte array, wire
    /// representation) into this set. Returns `true` iff this set was
    /// clear AND the merge introduced new bits (clean→dirty signal,
    /// race-tolerant).
    pub fn or_with(&self, other: &DiffMask) -> bool {
        let other_byte_count = other.byte_number() as usize;
        if other_byte_count == 0 {
            return false;
        }
        let was_clear_before = self.is_clear();
        let mut any_set = false;
        // Walk other's bytes, group into u64 words, OR into our words.
        for word_idx in 0..self.words.len() {
            let mut word_value = 0u64;
            let byte_base = word_idx * 8;
            for byte_offset in 0..8usize {
                let abs_byte = byte_base + byte_offset;
                if abs_byte >= other_byte_count {
                    break;
                }
                let byte = other.byte(abs_byte) as u64;
                if byte != 0 {
                    word_value |= byte << (byte_offset * 8);
                }
            }
            if word_value != 0 {
                any_set = true;
                self.words[word_idx].fetch_or(word_value, Ordering::Relaxed);
            }
        }
        was_clear_before && any_set
    }

    /// Snapshot the current bitset into an owned `DiffMask` (wire
    /// representation = `Vec<u8>`).
    pub fn snapshot(&self) -> DiffMask {
        let byte_n = self.byte_number();
        let mut mask = DiffMask::new(byte_n as u8);
        for (word_idx, word) in self.words.iter().enumerate() {
            let bits = word.load(Ordering::Relaxed);
            if bits == 0 {
                continue;
            }
            for byte_offset in 0..8usize {
                let abs_byte = word_idx * 8 + byte_offset;
                if abs_byte >= byte_n {
                    break;
                }
                let byte_val = ((bits >> (byte_offset * 8)) & 0xFF) as u8;
                if byte_val == 0 {
                    continue;
                }
                for bit in 0..8u8 {
                    if byte_val & (1 << bit) != 0 {
                        mask.set_bit((abs_byte as u8) * 8 + bit, true);
                    }
                }
            }
        }
        mask
    }

    /// Read one byte of the bitset (matches the `DiffMask::byte`
    /// little-endian byte layout).
    #[inline]
    pub fn byte(&self, index: usize) -> u8 {
        let word_idx = index / 8;
        let byte_in_word = index % 8;
        let Some(word) = self.words.get(word_idx) else {
            return 0;
        };
        let bits = word.load(Ordering::Relaxed);
        ((bits >> (byte_in_word * 8)) & 0xFF) as u8
    }

    /// Drain — atomically swap-zero every word and return the values
    /// (one `u64` per word, length = `words.len()`). Parallels
    /// `DirtyQueue::drain`, which inlines the same swap-zero loop for
    /// per-entity contiguous storage.
    #[allow(dead_code)] // Public API; no in-tree caller today.
    pub fn drain_words(&self) -> Vec<u64> {
        self.words
            .iter()
            .map(|w| w.swap(0, Ordering::Relaxed))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_zero_uses_one_word() {
        let m = AtomicBitSet::new(0);
        assert_eq!(m.words.len(), 1);
        assert_eq!(m.bit_capacity(), 0);
    }

    #[test]
    fn capacity_64_fits_one_word() {
        let m = AtomicBitSet::new(64);
        assert_eq!(m.words.len(), 1);
    }

    #[test]
    fn capacity_65_spills_to_two_words() {
        let m = AtomicBitSet::new(65);
        assert_eq!(m.words.len(), 2);
    }

    #[test]
    fn capacity_1024_uses_16_words() {
        let m = AtomicBitSet::new(1024);
        assert_eq!(m.words.len(), 16);
    }

    #[test]
    fn set_bit_first_returns_was_clear_true() {
        let m = AtomicBitSet::new(128);
        assert!(m.is_clear());
        assert!(m.set_bit(0));
        assert!(!m.set_bit(1));
        assert!(!m.set_bit(127));
    }

    #[test]
    fn set_bit_across_words_first_in_other_word_still_clean() {
        // First bit in word 0; clears all → was_clear true.
        let m = AtomicBitSet::new(128);
        assert!(m.set_bit(70)); // word 1
        // Second bit in same word: not was_clear.
        assert!(!m.set_bit(80));
        // Setting bit in word 0 while word 1 has bits: not was_clear.
        assert!(!m.set_bit(0));
    }

    #[test]
    fn clear_returns_was_dirty() {
        let m = AtomicBitSet::new(128);
        assert!(!m.clear());
        m.set_bit(70);
        assert!(m.clear());
        assert!(m.is_clear());
    }

    #[test]
    fn snapshot_round_trips_through_diff_mask() {
        let m = AtomicBitSet::new(256);
        for &bit in &[0u32, 7, 8, 63, 64, 127, 128, 255] {
            m.set_bit(bit);
        }
        let snap = m.snapshot();
        assert_eq!(snap.byte_number(), 32);
        for &bit in &[0u8, 7, 8, 63, 64, 127, 128, 255] {
            assert_eq!(snap.bit(bit), Some(true), "bit {} should be set", bit);
        }
    }

    #[test]
    fn out_of_range_set_bit_silent_no_op() {
        let m = AtomicBitSet::new(64);
        // bit 64 is out of range for a 64-bit capacity bitset.
        assert!(!m.set_bit(64));
        assert!(m.is_clear());
    }

    #[test]
    fn or_with_diff_mask_merges() {
        let m = AtomicBitSet::new(128);
        let mut other = DiffMask::new(16);
        other.set_bit(3, true);
        other.set_bit(70, true);
        assert!(m.or_with(&other));
        assert_eq!(m.byte(0), 0b0000_1000);
        assert_eq!(m.byte(8), 0b0100_0000);
    }

    #[test]
    fn or_with_zero_mask_noop() {
        let m = AtomicBitSet::new(128);
        let other = DiffMask::new(16);
        assert!(!m.or_with(&other));
        assert!(m.is_clear());
    }

    #[test]
    fn drain_words_returns_per_word_values_and_zeroes() {
        let m = AtomicBitSet::new(128);
        m.set_bit(3);
        m.set_bit(70);
        let drained = m.drain_words();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0], 1u64 << 3);
        assert_eq!(drained[1], 1u64 << 6);
        assert!(m.is_clear());
    }

    #[test]
    fn full_256_bits_supported() {
        let m = AtomicBitSet::new(256);
        for i in 0..256u32 {
            m.set_bit(i);
        }
        assert_eq!(m.byte_number(), 32);
        for i in 0..32 {
            assert_eq!(m.byte(i), 0xFF, "byte {} should be 0xFF", i);
        }
    }
}
