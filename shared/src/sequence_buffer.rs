use super::wrapping_number::{sequence_greater_than, sequence_less_than};
use std::ops::{Bound, RangeBounds};

/// Used to index packets that have been sent & received
pub type SequenceNumber = u16;

/// Collection to store data of any kind.
#[derive(Debug)]
pub struct SequenceBuffer<T> {
    sequence_num: SequenceNumber,
    entry_sequences: Box<[Option<SequenceNumber>]>,
    entries: Box<[Option<T>]>,
    newest_num: Option<SequenceNumber>,
    oldest_num: Option<SequenceNumber>,
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
            oldest_num: None,
            newest_num: None,
        }
    }

    /// Returns the most recently stored sequence number
    pub fn sequence_num(&self) -> SequenceNumber {
        self.sequence_num
    }

    /// Returns a mutable reference to the entry with the given sequence number.
    pub fn get_mut(&mut self, sequence_num: SequenceNumber) -> Option<&mut T> {
        if self.exists(sequence_num) {
            let index = self.index(sequence_num);
            return self.entries[index].as_mut();
        }
        None
    }

    /// Returns a reference to the entry with the given sequence number.
    pub fn get(&self, sequence_num: SequenceNumber) -> Option<&T> {
        if self.exists(sequence_num) {
            let index = self.index(sequence_num);
            return self.entries[index].as_ref();
        }
        None
    }

    /// Inserts the entry data into the sequence buffer. If the requested
    /// sequence number is "too old", the entry will not be inserted and will
    /// return false
    pub fn insert(&mut self, sequence_num: SequenceNumber, entry: T) -> bool {
        if !self.is_empty() {
            // sequence number is too old to insert into the buffer
            if sequence_less_than(
                sequence_num,
                self.sequence_num
                    .wrapping_sub(self.entry_sequences.len() as u16),
            ) {
                return false;
            }
        }

        self.advance_sequence(sequence_num);

        let index = self.index(sequence_num);
        self.entry_sequences[index] = Some(sequence_num);
        self.entries[index] = Some(entry);

        self.newest_num = Some(sequence_num);

        if self.oldest_num.is_none() {
            self.oldest_num = Some(sequence_num);
        }

        return true;
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
            if self.oldest_num.is_some() {
                if self.oldest_num.unwrap() == sequence_num {
                    self.oldest_num = None;

                    for i in 1..self.entry_sequences.len() as u16 {
                        let next_seq = sequence_num.wrapping_add(i);
                        if self.exists(next_seq) {
                            self.oldest_num = Some(next_seq);
                            break;
                        }
                    }
                }
            }

            if self.newest_num.is_some() {
                if self.newest_num.unwrap() == sequence_num {
                    self.newest_num = None;

                    for i in 1..self.entry_sequences.len() as u16 {
                        let next_seq = sequence_num.wrapping_sub(i);
                        if self.exists(next_seq) {
                            self.newest_num = Some(next_seq);
                            break;
                        }
                    }
                }
            }

            let index = self.index(sequence_num);
            let value = std::mem::replace(&mut self.entries[index], None);
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
                self.oldest_num = None;
                self.newest_num = None;
            }
        }
    }

    // Generates an index for use in `entry_sequences` and `entries`.
    pub fn index(&self, sequence: SequenceNumber) -> usize {
        sequence as usize % self.entry_sequences.len()
    }

    /// Gets the newest stored sequence number
    pub fn newest(&self) -> Option<SequenceNumber> {
        self.newest_num
    }

    /// Gets the oldest stored sequence number
    pub fn oldest(&self) -> Option<SequenceNumber> {
        self.oldest_num
    }

    /// Returns whether or not the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.oldest_num.is_none() && self.newest_num.is_none()
    }

    /// Remove entries up until a specific sequence number
    pub fn remove_until(&mut self, finish_sequence: u16) {
        if let Some(oldest_sequence) = self.oldest_num {
            for seq in oldest_sequence..finish_sequence {
                self.remove(seq);
            }
        }
    }

    /// Get a mutable iterator into the SequenceBuffer
    pub fn iter(&self, range: impl RangeBounds<SequenceNumber>) -> SequenceBufferIter<'_, T> {
        SequenceBufferIter::new(self, range)
    }

    /// Get a mutable iterator into the SequenceBuffer
    pub fn iter_mut(
        &mut self,
        range: impl RangeBounds<SequenceNumber>,
    ) -> SequenceBufferIterMut<'_, T> {
        SequenceBufferIterMut::new(self, range)
    }
}

// Iter
pub struct SequenceBufferIter<'b, T> {
    buffer: &'b SequenceBuffer<T>,
    start: SequenceNumber,
    end: SequenceNumber,
    ending: bool,
}

impl<'b, T> SequenceBufferIter<'b, T> {
    pub fn new(buffer: &'b SequenceBuffer<T>, range: impl RangeBounds<SequenceNumber>) -> Self {
        let start = get_start(range.start_bound(), buffer.oldest());
        let end = get_end(range.end_bound(), buffer.newest());

        SequenceBufferIter {
            buffer,
            start,
            end,
            ending: false,
        }
    }
}

impl<'b, T> Iterator for SequenceBufferIter<'b, T> {
    type Item = (SequenceNumber, &'b T);

    fn next(&mut self) -> Option<Self::Item> {
        if self.buffer.is_empty() {
            return None;
        }

        loop {
            if self.start == self.end {
                self.ending = true;
            }

            let current = self.start;
            self.start = self.start.wrapping_add(1);

            if let Some(item_mut) = self.buffer.get(current) {
                return Some((current, item_mut));
            }

            if self.ending {
                return None;
            }
        }
    }
}

// IterMut
pub struct SequenceBufferIterMut<'b, T> {
    buffer: &'b mut SequenceBuffer<T>,
    start: SequenceNumber,
    end: SequenceNumber,
    ending: bool,
}

impl<'b, T> SequenceBufferIterMut<'b, T> {
    pub fn new(buffer: &'b mut SequenceBuffer<T>, range: impl RangeBounds<SequenceNumber>) -> Self {
        let start = get_start(range.start_bound(), buffer.oldest());
        let end = get_end(range.end_bound(), buffer.newest());

        SequenceBufferIterMut {
            buffer,
            start,
            end,
            ending: false,
        }
    }
}

impl<'b, T> Iterator for SequenceBufferIterMut<'b, T> {
    type Item = SequenceBufferItemMut<'b, T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.buffer.is_empty() {
            return None;
        }

        loop {
            if self.start == self.end {
                self.ending = true;
            }
            let current = self.start;
            self.start = self.start.wrapping_add(1);

            let index = self.buffer.index(current);
            if self.buffer.entries[index].is_some() {
                let ptr = self.buffer.entries.as_mut_ptr();
                unsafe {
                    let ptr_opt = &mut *ptr.add(index);
                    let ptr_unwrapped = ptr_opt.as_mut().unwrap();
                    return Some(SequenceBufferItemMut {
                        index: current,
                        item: ptr_unwrapped,
                    });
                }
            }

            if self.ending {
                return None;
            }
        }
    }
}

pub struct SequenceBufferItemMut<'i, T> {
    pub index: SequenceNumber,
    pub item: &'i mut T,
}

fn get_start(
    start_bound: Bound<&SequenceNumber>,
    oldest: Option<SequenceNumber>,
) -> SequenceNumber {
    match start_bound {
        Bound::Excluded(seq) => seq.wrapping_add(1),
        Bound::Included(seq) => *seq,
        Bound::Unbounded => {
            if let Some(seq) = oldest {
                seq
            } else {
                0
            }
        }
    }
}

fn get_end(end_bound: Bound<&SequenceNumber>, newest: Option<SequenceNumber>) -> SequenceNumber {
    match end_bound {
        Bound::Excluded(seq) => seq.wrapping_sub(1),
        Bound::Included(seq) => *seq,
        Bound::Unbounded => {
            if let Some(seq) = newest {
                seq
            } else {
                0
            }
        }
    }
}
