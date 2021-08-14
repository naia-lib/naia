use std::{collections::HashMap, rc::Rc};

use naia_shared::{State, EventClone, StateType, PawnKey};

/// Handles outgoing Commands
#[derive(Debug)]
pub struct CommandSender<T: StateType> {
    queued_outgoing_command: HashMap<PawnKey, Rc<Box<dyn State<T>>>>,
}

impl<T: StateType> CommandSender<T> {
    /// Creates a new CommandSender
    pub fn new() -> Self {
        CommandSender {
            queued_outgoing_command: HashMap::new(),
        }
    }

    /// Gets the next queued Command to be transmitted
    pub fn has_command(&self) -> bool {
        self.queued_outgoing_command.len() != 0
    }

    /// Gets the next queued Command to be transmitted
    pub fn pop_command(&mut self) -> Option<(PawnKey, Rc<Box<dyn State<T>>>)> {
        let mut out_key = None;
        if let Some((key, _)) = self.queued_outgoing_command.iter().next() {
            out_key = Some(*key);
        }

        if let Some(key) = out_key {
            if let Some(command) = self.queued_outgoing_command.remove(&key) {
                return Some((key, command));
            }
        }
        return None;
    }

    /// If  the last popped Command from the queue somehow wasn't able to be
    /// written into a packet, put the Command back into the front of the queue
    pub fn unpop_command(&mut self, pawn_key: &PawnKey, command: &Rc<Box<dyn State<T>>>) {
        let cloned_command = command.clone();
        self.queued_outgoing_command
            .insert(*pawn_key, cloned_command);
    }

    /// Queues an Command to be transmitted to the remote host
    pub fn queue_command(&mut self, pawn_key: &PawnKey, command: &impl State<T>) {
        let cloned_command = Rc::new(EventClone::clone_box(command));
        self.queued_outgoing_command
            .insert(*pawn_key, cloned_command);
    }
}
