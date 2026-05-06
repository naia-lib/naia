//! Lock-free atomic dirty-bit mask for [`crate::MutReceiver`].
//!
//! One bit per `Property<T>` field on a component. Replaces the old
//! `RwLock<DiffMask>` mutation hot path with a `fetch_or` on the
//! underlying [`AtomicBitSet`].
//!
//! ## Width
//!
//! Variable-width via `AtomicBitSet`'s `Box<[AtomicU64]>` storage —
//! no upper limit on the number of properties per component. The
//! historical "≤8 bytes / ≤64 properties" cap (which previously panicked
//! with a `debug_assert!`) is gone.

use crate::world::update::atomic_bit_set::AtomicBitSet;
use crate::DiffMask;

/// Lock-free dirty-bit mask, one bit per `Property<T>` field.
pub struct AtomicDiffMask {
    bits: AtomicBitSet,
}

impl AtomicDiffMask {
    /// Construct with capacity for `byte_len` bytes (= `byte_len * 8`
    /// bit positions). The wire format encodes the mask as a
    /// little-endian byte array of this length.
    pub fn new(byte_len: u8) -> Self {
        Self {
            bits: AtomicBitSet::new((byte_len as usize) * 8),
        }
    }

    /// Set the bit at `index`. Returns `true` iff the entire mask was
    /// clear before this call (the signal `MutReceiver` uses to fire
    /// `notify_dirty` exactly once per clean→dirty transition).
    #[inline]
    pub fn set_bit(&self, index: u8) -> bool {
        self.bits.set_bit(index as u32)
    }

    /// Clear all bits. Returns `true` iff the mask had any bit set,
    /// the signal `MutReceiver` uses to fire `notify_clean` exactly
    /// once per dirty→clean transition.
    #[inline]
    pub fn clear(&self) -> bool {
        self.bits.clear()
    }

    /// OR-merge another mask into this one (used by retransmit on
    /// packet drop in `EntityUpdateManager::dropped_update_cleanup`).
    /// Returns `true` iff this mask was clear AND the merge introduced
    /// new bits.
    pub fn or_with(&self, other: &DiffMask) -> bool {
        self.bits.or_with(other)
    }

    /// Snapshot the current mask into an owned `DiffMask`. Used by
    /// `world_writer` when copying the mask into `sent_updates` before
    /// clearing.
    pub fn snapshot(&self) -> DiffMask {
        self.bits.snapshot()
    }

    #[inline]
    pub fn is_clear(&self) -> bool {
        self.bits.is_clear()
    }

    /// Read one byte of the mask. Cheaper than `snapshot()` when the
    /// caller only needs a single byte.
    #[inline]
    pub fn byte(&self, index: usize) -> u8 {
        self.bits.byte(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_bit_round_trips_through_snapshot() {
        let m = AtomicDiffMask::new(1);
        assert!(m.is_clear());
        let was_clear = m.set_bit(2);
        assert!(was_clear);
        assert!(!m.is_clear());
        let snap = m.snapshot();
        assert_eq!(snap.byte(0), 0b0000_0100);
        assert_eq!(snap.byte_number(), 1);
    }

    #[test]
    fn set_bit_returns_was_clear_only_on_first_transition() {
        let m = AtomicDiffMask::new(1);
        assert!(m.set_bit(0));
        assert!(!m.set_bit(1));
        assert!(!m.set_bit(7));
    }

    #[test]
    fn clear_returns_was_dirty() {
        let m = AtomicDiffMask::new(1);
        assert!(!m.clear());
        m.set_bit(3);
        assert!(m.clear());
        assert!(m.is_clear());
    }

    #[test]
    fn or_with_merges_and_signals_was_clear() {
        let m = AtomicDiffMask::new(2);
        let mut other = DiffMask::new(2);
        other.set_bit(1, true);
        other.set_bit(9, true);
        assert!(m.or_with(&other));
        assert_eq!(m.byte(0), 0b0000_0010);
        assert_eq!(m.byte(1), 0b0000_0010);
        let mut other2 = DiffMask::new(2);
        other2.set_bit(2, true);
        assert!(!m.or_with(&other2));
        assert_eq!(m.byte(0), 0b0000_0110);
    }

    #[test]
    fn or_with_zero_mask_is_noop_and_returns_false() {
        let m = AtomicDiffMask::new(1);
        let other = DiffMask::new(1);
        assert!(!m.or_with(&other));
        assert!(m.is_clear());
    }

    #[test]
    fn byte_layout_matches_diff_mask_byte_for_byte() {
        let mut reference = DiffMask::new(2);
        let atomic = AtomicDiffMask::new(2);
        for &bit in &[0u8, 3, 7, 8, 11, 15] {
            reference.set_bit(bit, true);
            atomic.set_bit(bit);
        }
        for i in 0..2 {
            assert_eq!(atomic.byte(i), reference.byte(i), "byte {} differs", i);
        }
        assert_eq!(atomic.snapshot(), reference);
    }

    #[test]
    fn full_64_bits_supported() {
        let m = AtomicDiffMask::new(8);
        for i in 0..64u8 {
            m.set_bit(i);
        }
        for i in 0..8 {
            assert_eq!(m.byte(i), 0xFF);
        }
    }

    /// >64-bit mask used to panic with `debug_assert!`. Now supported
    /// > via the multi-word AtomicBitSet backing.
    #[test]
    fn over_64_bits_supported_no_more_8_byte_limit() {
        let m = AtomicDiffMask::new(32); // 256 bits / 256 properties
        for i in 0..255u8 {
            m.set_bit(i);
        }
        // Set bit 255 (last addressable in u8) — the property index API
        // is u8 so this is the practical max for a single component.
        m.set_bit(255);
        for i in 0..32 {
            assert_eq!(m.byte(i), 0xFF, "byte {} should be 0xFF", i);
        }
        assert_eq!(m.snapshot().byte_number(), 32);
    }
}
