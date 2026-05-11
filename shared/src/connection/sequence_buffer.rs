use crate::wrapping_number::{sequence_greater_than, sequence_less_than};

/// Used to index packets that have been sent & received
pub type SequenceNumber = u16;

/// Collection to store data of any kind.
pub struct SequenceBuffer<T> {
    sequence_num: SequenceNumber,
    entry_sequences: Box<[Option<SequenceNumber>]>,
    entries: Box<[Option<T>]>,
}

impl<T> SequenceBuffer<T> {
    /// Creates a SequenceBuffer with a desired capacity.
    pub fn with_capacity(size: u16) -> Self {
        let mut entries = Vec::<Option<T>>::new();
        for _ in 0..size {
            entries.push(None);
        }
        Self {
            sequence_num: 0,
            entry_sequences: vec![None; size as usize].into_boxed_slice(),
            entries: entries.into_boxed_slice(),
        }
    }

    /// Returns the most recently stored sequence number.
    #[allow(dead_code)] // used in unit tests; no production call site needed
    pub fn sequence_num(&self) -> SequenceNumber {
        self.sequence_num
    }

    /// Inserts the entry data into the sequence buffer. If the requested
    /// sequence number is "too old", the entry will not be inserted and will
    /// return false
    pub fn insert(&mut self, sequence_num: SequenceNumber, entry: T) -> bool {
        // sequence number is too old to insert into the buffer
        if sequence_less_than(
            sequence_num,
            self.sequence_num
                .wrapping_sub(self.entry_sequences.len() as u16),
        ) {
            return false;
        }

        self.advance_sequence(sequence_num);

        let index = self.index(sequence_num);
        self.entry_sequences[index] = Some(sequence_num);
        self.entries[index] = Some(entry);

        true
    }

    /// Returns whether or not we have previously inserted an entry for the
    /// given sequence number.
    pub fn exists(&self, sequence_num: SequenceNumber) -> bool {
        let index = self.index(sequence_num);
        if let Some(s) = self.entry_sequences[index] {
            return s == sequence_num;
        }
        false
    }

    /// Removes an entry from the sequence buffer
    pub fn remove(&mut self, sequence_num: SequenceNumber) -> Option<T> {
        if self.exists(sequence_num) {
            let index = self.index(sequence_num);
            let value = self.entries[index].take();
            self.entry_sequences[index] = None;
            return value;
        }
        None
    }

    // Advances the sequence number while removing older entries.
    fn advance_sequence(&mut self, sequence_num: SequenceNumber) {
        if sequence_greater_than(sequence_num.wrapping_add(1), self.sequence_num) {
            self.remove_entries(u32::from(sequence_num));
            self.sequence_num = sequence_num.wrapping_add(1);
        }
    }

    fn remove_entries(&mut self, mut finish_sequence: u32) {
        let start_sequence = u32::from(self.sequence_num);
        if finish_sequence < start_sequence {
            finish_sequence += 65536;
        }

        if finish_sequence - start_sequence < self.entry_sequences.len() as u32 {
            for sequence in start_sequence..=finish_sequence {
                self.remove(sequence as u16);
            }
        } else {
            for index in 0..self.entry_sequences.len() {
                self.entries[index] = None;
                self.entry_sequences[index] = None;
            }
        }
    }

    // Generates an index for use in `entry_sequences` and `entries`.
    fn index(&self, sequence: SequenceNumber) -> usize {
        sequence as usize % self.entry_sequences.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Basic insert/exists/remove ────────────────────────────────────────────

    #[test]
    fn insert_and_exists() {
        let mut buf = SequenceBuffer::<u8>::with_capacity(32);
        assert!(buf.insert(0, 42));
        assert!(buf.exists(0));
        assert!(!buf.exists(1));
    }

    #[test]
    fn remove_returns_value() {
        let mut buf = SequenceBuffer::<u8>::with_capacity(32);
        buf.insert(5, 99);
        assert_eq!(buf.remove(5), Some(99));
        assert!(!buf.exists(5));
        assert_eq!(buf.remove(5), None);
    }

    #[test]
    fn too_old_insert_rejected() {
        let mut buf = SequenceBuffer::<u8>::with_capacity(8);
        // After inserting 100, sequence_num becomes 101.
        // Rejection threshold = sequence_num - capacity = 101 - 8 = 93.
        // Sequences < 93 are rejected; 93 and above are accepted.
        buf.insert(100, 1);
        assert!(!buf.insert(92, 2), "92 < threshold 93: should be rejected");
        assert!(buf.insert(93, 3), "93 == threshold: should be accepted");
    }

    // ── u16 wraparound ────────────────────────────────────────────────────────

    #[test]
    fn insert_wraps_around_u16_max() {
        let mut buf = SequenceBuffer::<u8>::with_capacity(64);
        // Insert near the u16 max
        assert!(buf.insert(65534, 10));
        assert!(buf.insert(65535, 20));
        // Wrap: 0 is the next sequence after 65535
        assert!(buf.insert(0, 30));
        assert!(buf.insert(1, 40));

        assert!(buf.exists(65534));
        assert!(buf.exists(65535));
        assert!(buf.exists(0));
        assert!(buf.exists(1));
    }

    #[test]
    fn wraparound_evicts_old_entries() {
        let mut buf = SequenceBuffer::<u8>::with_capacity(4);
        buf.insert(65534, 1);
        buf.insert(65535, 2);
        buf.insert(0, 3); // wraps; 65534 is now capacity-entries old
        buf.insert(1, 4); // wraps; 65535 is now capacity-entries old
        buf.insert(2, 5); // should evict 65534 (4 slots back)

        assert!(!buf.exists(65534), "65534 should be evicted");
        assert!(buf.exists(0));
        assert!(buf.exists(1));
        assert!(buf.exists(2));
    }

    #[test]
    fn sequence_num_wraps_correctly() {
        let mut buf = SequenceBuffer::<()>::with_capacity(8);
        buf.insert(65535, ());
        // After inserting 65535, sequence_num should be 0 (wrapping_add(1))
        assert_eq!(buf.sequence_num(), 0);
    }

    // ── Index stability ───────────────────────────────────────────────────────

    #[test]
    fn index_is_modulo_capacity() {
        // Verify two sequence numbers that map to the same index don't interfere
        // after one is evicted.
        let mut buf = SequenceBuffer::<u8>::with_capacity(4);
        buf.insert(0, 100);
        buf.insert(1, 101);
        buf.insert(2, 102);
        buf.insert(3, 103);
        // Advance past capacity — entries 0..3 evicted
        buf.insert(4, 104);
        assert!(!buf.exists(0), "seq 0 evicted by 4 (same slot)");
        assert!(buf.exists(4));
    }
}
