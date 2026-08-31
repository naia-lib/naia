use std::collections::VecDeque;

use crate::{sequence_less_than, MessageIndex};

pub struct OrderedIds<P> {
    // front small, back big
    inner: VecDeque<(MessageIndex, P)>,
}

impl<P> OrderedIds<P> {
    pub fn new() -> Self {
        Self {
            inner: VecDeque::new(),
        }
    }

    // pub fn push_front(&mut self, index: MessageIndex) {
    //     let mut index = 0;
    //
    //     loop {
    //         if index == self.inner.len() {
    //             self.inner.push_back(index);
    //             return;
    //         }
    //
    //         let old_index = self.inner.get(index).unwrap();
    //         if sequence_greater_than(*old_index, index) {
    //             self.inner.insert(index, index);
    //             return;
    //         }
    //
    //         index += 1
    //     }
    // }

    pub fn push_back(&mut self, message_index: MessageIndex, item: P) {
        let mut current_index = self.inner.len();

        loop {
            if current_index == 0 {
                self.inner.push_front((message_index, item));
                return;
            }

            current_index -= 1;

            let (old_index, _) = self.inner.get(current_index).unwrap();
            if sequence_less_than(*old_index, message_index) {
                self.inner.insert(current_index + 1, (message_index, item));
                return;
            }
        }
    }

    pub fn peek_front(&self) -> Option<&(MessageIndex, P)> {
        self.inner.front()
    }

    pub fn pop_front(&mut self) -> Option<(MessageIndex, P)> {
        self.inner.pop_front()
    }

    pub fn pop_front_until_and_including(&mut self, index: MessageIndex) {
        self.pop_front_until(index, true);
    }

    pub fn pop_front_until_and_excluding(&mut self, index: MessageIndex) {
        self.pop_front_until(index, false);
    }

    fn pop_front_until(&mut self, index: MessageIndex, including: bool) {
        loop {
            let Some((old_index, _)) = self.inner.front() else {
                return;
            };
            let old_index = *old_index;
            if sequence_less_than(old_index, index) || (including && old_index == index) {
                self.inner.pop_front();
            } else {
                return;
            }
        }
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }

    #[cfg(feature = "e2e_debug")]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[cfg(feature = "e2e_debug")]
    pub fn find_by_predicate<F: Fn(&P) -> bool>(&self, predicate: F) -> Option<(MessageIndex, P)>
    where
        P: Clone,
    {
        self.inner
            .iter()
            .find(|(_, item)| predicate(item))
            .map(|(id, item)| (*id, item.clone()))
    }
}

#[cfg(test)]
mod tests {
    //! `OrderedIds` is the ordered buffer every remote channel holds its
    //! pending messages in, and it had no tests at all. Two survivors of the
    //! first sweep of this file were real: `pop_front_until_and_excluding` and
    //! `clear` could each be replaced with `()` unnoticed.
    //!
    //! Everything here is ordered by `sequence_less_than`, not by `<`: message
    //! indices are a wrapping u16 sequence, so an id numerically smaller than
    //! the front can still be *later* in the sequence. The wrap cases below are
    //! the ones a plain integer comparison gets wrong.

    use super::*;

    fn drain(ids: &mut OrderedIds<char>) -> Vec<(MessageIndex, char)> {
        let mut out = Vec::new();
        while let Some(entry) = ids.pop_front() {
            out.push(entry);
        }
        out
    }

    fn from_pairs(pairs: &[(MessageIndex, char)]) -> OrderedIds<char> {
        let mut ids = OrderedIds::new();
        for (index, item) in pairs {
            ids.push_back(*index, *item);
        }
        ids
    }

    #[test]
    fn an_empty_buffer_yields_nothing() {
        let mut ids: OrderedIds<char> = OrderedIds::new();

        assert!(ids.peek_front().is_none());
        assert!(ids.pop_front().is_none());
    }

    #[test]
    fn items_come_back_in_sequence_order_however_they_went_in() {
        let mut ids = from_pairs(&[(3, 'c'), (1, 'a'), (4, 'd'), (2, 'b')]);

        assert_eq!(
            drain(&mut ids),
            vec![(1, 'a'), (2, 'b'), (3, 'c'), (4, 'd')],
        );
    }

    #[test]
    fn the_front_is_the_lowest_id_without_removing_it() {
        let mut ids = from_pairs(&[(5, 'b'), (2, 'a')]);

        assert_eq!(ids.peek_front(), Some(&(2, 'a')));
        assert_eq!(ids.peek_front(), Some(&(2, 'a')), "peek must not consume");
        assert_eq!(ids.pop_front(), Some((2, 'a')));
    }

    /// Indices wrap: 65535 comes *before* 0 in the sequence, so an id that is
    /// numerically far larger sorts to the front.
    #[test]
    fn ordering_follows_the_wrapping_sequence_not_the_raw_number() {
        let mut ids = from_pairs(&[(1, 'c'), (65535, 'a'), (0, 'b')]);

        assert_eq!(
            drain(&mut ids),
            vec![(65535, 'a'), (0, 'b'), (1, 'c')],
            "the buffer sorted by raw integer value instead of sequence order",
        );
    }

    #[test]
    fn popping_until_and_excluding_leaves_the_named_id_in_place() {
        let mut ids = from_pairs(&[(1, 'a'), (2, 'b'), (3, 'c')]);

        ids.pop_front_until_and_excluding(2);

        assert_eq!(
            drain(&mut ids),
            vec![(2, 'b'), (3, 'c')],
            "the boundary id itself must survive an exclusive pop",
        );
    }

    #[test]
    fn popping_until_and_including_removes_the_named_id_too() {
        let mut ids = from_pairs(&[(1, 'a'), (2, 'b'), (3, 'c')]);

        ids.pop_front_until_and_including(2);

        assert_eq!(drain(&mut ids), vec![(3, 'c')]);
    }

    /// The two variants must differ on exactly one entry -- the boundary. Run
    /// side by side, this pins the `including` flag itself: make either
    /// wrapper delegate with the wrong flag and the pair stops holding.
    #[test]
    fn including_and_excluding_differ_only_at_the_boundary() {
        let mut inclusive = from_pairs(&[(1, 'a'), (2, 'b'), (3, 'c')]);
        let mut exclusive = from_pairs(&[(1, 'a'), (2, 'b'), (3, 'c')]);

        inclusive.pop_front_until_and_including(2);
        exclusive.pop_front_until_and_excluding(2);

        assert_eq!(drain(&mut inclusive), vec![(3, 'c')]);
        assert_eq!(drain(&mut exclusive), vec![(2, 'b'), (3, 'c')]);
    }

    #[test]
    fn popping_stops_at_the_first_id_past_the_boundary() {
        let mut ids = from_pairs(&[(1, 'a'), (5, 'b'), (2, 'c')]);

        ids.pop_front_until_and_including(3);

        assert_eq!(
            drain(&mut ids),
            vec![(5, 'b')],
            "everything up to 3 goes, and the scan stops rather than \
             continuing past a surviving entry",
        );
    }

    #[test]
    fn popping_past_the_end_empties_the_buffer_without_panicking() {
        let mut ids = from_pairs(&[(1, 'a'), (2, 'b')]);

        ids.pop_front_until_and_including(99);

        assert!(ids.peek_front().is_none());
    }

    #[test]
    fn popping_below_the_front_removes_nothing() {
        let mut ids = from_pairs(&[(5, 'a'), (6, 'b')]);

        ids.pop_front_until_and_including(2);

        assert_eq!(drain(&mut ids), vec![(5, 'a'), (6, 'b')]);
    }

    #[test]
    fn popping_an_empty_buffer_is_a_no_op() {
        let mut ids: OrderedIds<char> = OrderedIds::new();

        ids.pop_front_until_and_including(7);
        ids.pop_front_until_and_excluding(7);

        assert!(ids.peek_front().is_none());
    }

    #[test]
    fn clearing_discards_everything() {
        let mut ids = from_pairs(&[(1, 'a'), (2, 'b'), (3, 'c')]);

        ids.clear();

        assert!(ids.peek_front().is_none(), "clear left entries behind",);
        assert!(ids.pop_front().is_none());
    }

    /// A cleared buffer must still be usable -- and must not have kept any
    /// ordering state that would misplace the next push.
    #[test]
    fn a_cleared_buffer_can_be_refilled() {
        let mut ids = from_pairs(&[(4, 'a'), (5, 'b')]);
        ids.clear();

        ids.push_back(2, 'c');
        ids.push_back(1, 'd');

        assert_eq!(drain(&mut ids), vec![(1, 'd'), (2, 'c')]);
    }

    #[test]
    fn duplicate_ids_are_both_kept() {
        let mut ids = from_pairs(&[(1, 'a'), (1, 'b')]);

        assert_eq!(
            drain(&mut ids).len(),
            2,
            "the buffer must not silently deduplicate; a repeated index is the \
             caller's problem to notice, not this structure's to hide",
        );
    }
}
