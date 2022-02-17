use std::collections::VecDeque;
use std::iter::Rev;

use naia_client::shared::{sequence_greater_than, sequence_less_than};

type Index = u16;

pub struct CommandHistory<T> {
    buffer: VecDeque<(Index, T)>
}

impl<T> CommandHistory<T> {
    pub fn new(capacity: usize) -> Self {
        CommandHistory {
            buffer: VecDeque::new(),
        }
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut (Index, T)> {
        return self.buffer.iter_mut().rev();
    }

    // this only goes forward
    pub fn push_front(&mut self, index: Index, command: T) {
        let mut some_front_index = None;
        if let Some((front_index, front_command)) = self.buffer.front() {
            some_front_index = Some(*front_index);
        }
        if let Some(front_index) = some_front_index {
            if sequence_less_than(index, front_index) {
                panic!("Can't push a lesser index to the Command Buffer!");
            }
        }

        // go ahead and push
        self.buffer.push_front((index, command));
    }

    pub fn remove_until(&mut self, index: Index) {
        loop {
            let mut some_back_index = None;
            if let Some((back_index, back_command)) = self.buffer.back() {
                some_back_index = Some(*back_index);
            } else {
                return;
            }
            let back_index = some_back_index.unwrap();
            if back_index == index || sequence_greater_than(back_index, index) {
                return;
            }
            self.buffer.pop_back();
        }
    }
}