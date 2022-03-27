use std::collections::VecDeque;

use naia_client::shared::{sequence_greater_than, sequence_less_than};

type Tick = u16;

pub struct CommandHistory<T> {
    buffer: VecDeque<(Tick, T)>,
}

impl<T> CommandHistory<T> {
    pub fn new() -> Self {
        CommandHistory {
            buffer: VecDeque::new(),
        }
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut (Tick, T)> {
        return self.buffer.iter_mut().rev();
    }

    // this only goes forward
    pub fn insert(&mut self, new_command_tick: Tick, new_command: T) {
        if let Some((last_most_recent_command_tick, _)) = self.buffer.front() {
            if !sequence_greater_than(new_command_tick, *last_most_recent_command_tick) {
                panic!("You must always insert a more recent command into the CommandHistory than the one you last inserted.");
            }
        }

        // go ahead and push
        self.buffer.push_front((new_command_tick, new_command));
    }

    pub fn remove_to_and_including(&mut self, index: Tick) {
        loop {
            let back_index = match self.buffer.back() {
                Some((index, _)) => *index,
                None => {
                    return;
                }
            };
            if sequence_greater_than(back_index, index) {
                return;
            }
            self.buffer.pop_back();
        }
    }

    pub fn can_insert(&self, tick: &Tick) -> bool {
        if let Some((last_most_recent_command_tick, _)) = self.buffer.front() {
            if !sequence_greater_than(*tick, *last_most_recent_command_tick) {
                return false;
            }
        }
        return true;
    }
}
