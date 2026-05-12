use std::collections::VecDeque;

use naia_shared::{sequence_greater_than, Tick};

/// Ring buffer of (tick, command) pairs for client-prediction rollback; old entries are pruned when the server acknowledges a tick.
pub struct CommandHistory<T: Clone> {
    buffer: VecDeque<(Tick, T)>,
}

impl<T: Clone> Default for CommandHistory<T> {
    fn default() -> Self {
        Self {
            buffer: VecDeque::default(),
        }
    }
}

impl<T: Clone> CommandHistory<T> {
    /// Drops all history up to and including `start_tick`, then returns all remaining (tick, command) pairs for replay.
    pub fn replays(&mut self, start_tick: &Tick) -> Vec<(Tick, T)> {
        // Remove history of commands until current received tick
        self.remove_to_and_including(*start_tick);

        // Get copies of all remaining stored Commands
        let mut output = Vec::new();

        for (tick, command) in self.buffer.iter() {
            output.push((*tick, command.clone()));
        }

        output
    }

    /// Appends `new_command` at `command_tick`; panics if `command_tick` is not strictly later than the last inserted tick.
    // this only goes forward
    pub fn insert(&mut self, command_tick: Tick, new_command: T) {
        if let Some((last_most_recent_command_tick, _)) = self.buffer.back() {
            if !sequence_greater_than(command_tick, *last_most_recent_command_tick) {
                panic!("You must always insert a more recent command into the CommandHistory than the one you last inserted.");
            }
        }

        // go ahead and push
        self.buffer.push_back((command_tick, new_command));
    }

    fn remove_to_and_including(&mut self, index: Tick) {
        loop {
            let back_index = match self.buffer.front() {
                Some((index, _)) => *index,
                None => {
                    return;
                }
            };
            if sequence_greater_than(back_index, index) {
                return;
            }
            self.buffer.pop_front();
        }
    }

    /// Returns `true` if `tick` is strictly later than the most-recently inserted tick, meaning a new command can be appended.
    pub fn can_insert(&self, tick: &Tick) -> bool {
        if let Some((last_most_recent_command_tick, _)) = self.buffer.back() {
            if !sequence_greater_than(*tick, *last_most_recent_command_tick) {
                return false;
            }
        }
        true
    }

    /// Returns the tick of the most-recently buffered command, or `None` if the buffer is empty.
    pub fn most_recent_tick(&self) -> Option<Tick> {
        self.buffer.back().map(|(tick, _)| *tick)
    }
}
