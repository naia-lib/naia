use std::sync::atomic::{AtomicU64, Ordering};

use crate::DiffMask;

/// Lock-free DiffMask cell used by [`MutReceiver`].
///
/// Replaces `RwLock<DiffMask>` on the per-receiver mutation hot path. A
/// component update is one bit; the variable-length wire encoding
/// addresses these bits as a little-endian byte array. We pack the whole
/// mask into a single `AtomicU64` (8 bytes / 64 bits), which covers every
/// production component shape — the wire-format `byte_number` is a `u8`
/// but components in cyberlith / common netgame uses sit at ≤ 8 props
/// each. Anything wider would need a `Box<[AtomicU64]>` fallback; we'll
/// add that when the first such component appears, not before.
///
/// Wire layout: bit `i` lives at `(byte_index = i / 8, bit_in_byte = i % 8)`,
/// so byte `b` of the mask is `((mask.load() >> (b * 8)) & 0xFF) as u8`.
/// This matches `DiffMask`'s `Vec<u8>`-LE layout byte-for-byte.
pub struct AtomicDiffMask {
    /// Packed bits. Bit position = `byte_index * 8 + bit_in_byte`.
    /// Stored as `u64` for one-shot atomic mutation.
    bits: AtomicU64,
    /// Number of mask bytes the wire format expects. Constant for the
    /// lifetime of the receiver — `≤ 8`. Stored so `byte_number()` can
    /// return the wire-correct value rather than always 8.
    byte_len: u8,
}

impl AtomicDiffMask {
    pub fn new(byte_len: u8) -> Self {
        debug_assert!(
            byte_len <= 8,
            "AtomicDiffMask supports up to 8 bytes (64 bits); component has too many properties — \
             add a Box<[AtomicU64]> fallback to support >64 props"
        );
        Self {
            bits: AtomicU64::new(0),
            byte_len,
        }
    }

    /// Set the bit at `index`. Returns `true` iff the entire mask was clear
    /// before this call — the signal `MutReceiver` uses to fire `notify_dirty`
    /// exactly once per clean → dirty transition.
    #[inline]
    pub fn set_bit(&self, index: u8) -> bool {
        let bit = 1u64 << (index as u32);
        let prev = self.bits.fetch_or(bit, Ordering::Relaxed);
        prev == 0
    }

    /// Clear all bits. Returns `true` iff the mask had any bit set, the
    /// signal `MutReceiver` uses to fire `notify_clean` exactly once per
    /// dirty → clean transition.
    #[inline]
    pub fn clear(&self) -> bool {
        let prev = self.bits.swap(0, Ordering::Relaxed);
        prev != 0
    }

    /// OR-merge another mask into this one (used by retransmit on packet
    /// drop in `EntityUpdateManager::dropped_update_cleanup`). Returns
    /// `true` iff this mask was clear AND the merge introduced new bits —
    /// the same `notify_dirty`-once contract as `set_bit`.
    pub fn or_with(&self, other: &DiffMask) -> bool {
        let other_bits = pack_diff_mask(other);
        if other_bits == 0 {
            return false;
        }
        let prev = self.bits.fetch_or(other_bits, Ordering::Relaxed);
        prev == 0
    }

    /// Snapshot the current mask into an owned `DiffMask`. Used by
    /// `world_writer` when copying the mask into `sent_updates` before
    /// clearing — the recorded copy survives subsequent mutations and is
    /// what `dropped_update_cleanup` ORs back on retransmit.
    pub fn snapshot(&self) -> DiffMask {
        let bits = self.bits.load(Ordering::Relaxed);
        unpack_diff_mask(bits, self.byte_len)
    }

    #[inline]
    pub fn is_clear(&self) -> bool {
        self.bits.load(Ordering::Relaxed) == 0
    }

    /// Read one byte of the mask. Equivalent to `snapshot().byte(i)` but
    /// avoids the `DiffMask` allocation when callers only need one byte.
    #[inline]
    pub fn byte(&self, index: usize) -> u8 {
        let bits = self.bits.load(Ordering::Relaxed);
        ((bits >> (index * 8)) & 0xFF) as u8
    }
}

fn pack_diff_mask(mask: &DiffMask) -> u64 {
    let bytes = mask.byte_number();
    let n = bytes.min(8) as usize;
    let mut out = 0u64;
    for i in 0..n {
        out |= (mask.byte(i) as u64) << (i * 8);
    }
    out
}

fn unpack_diff_mask(bits: u64, byte_len: u8) -> DiffMask {
    let mut mask = DiffMask::new(byte_len);
    for byte_idx in 0..byte_len as usize {
        let byte = ((bits >> (byte_idx * 8)) & 0xFF) as u8;
        if byte == 0 {
            continue;
        }
        for bit in 0..8u8 {
            if byte & (1 << bit) != 0 {
                mask.set_bit((byte_idx as u8) * 8 + bit, true);
            }
        }
    }
    mask
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
        // OR-with on already-dirty mask returns false even when more bits land.
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
        // Wire-format contract: AtomicDiffMask.byte(i) must equal
        // DiffMask.byte(i) for every i, given the same logical bit set.
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
}
