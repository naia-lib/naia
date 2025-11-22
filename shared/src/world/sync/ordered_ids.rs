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
}
