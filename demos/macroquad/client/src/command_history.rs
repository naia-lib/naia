use std::collections::VecDeque;

use naia_client::shared::{sequence_greater_than, sequence_less_than};

type Index = u16;

pub struct CommandHistory<T> {
    buffer: VecDeque<(Index, T)>
}

impl<T> CommandHistory<T> {
    pub fn new() -> Self {
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
        if let Some((front_index, _)) = self.buffer.front() {
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

    pub fn remove_to_and_including(&mut self, index: Index) {
        loop {
            let back_index = match self.buffer.back() {
                Some((index, _)) => *index,
                None => { return; }
            };
            if sequence_greater_than(back_index, index) {
                return;
            }
            self.buffer.pop_back();
        }
    }
}