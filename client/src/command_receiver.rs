use std::{collections::VecDeque, rc::Rc};

use crate::naia_client::LocalEntityKey;
use naia_shared::{Event, EventClone, EventType};

/// Handles incoming, local, predicted Commands
#[derive(Debug)]
pub struct CommandReceiver<T: EventType> {
    queued_incoming_commands: VecDeque<(LocalEntityKey, Rc<Box<dyn Event<T>>>)>,
}

impl<T: EventType> CommandReceiver<T> {
    /// Creates a new CommandSender
    pub fn new() -> Self {
        CommandReceiver {
            queued_incoming_commands: VecDeque::new(),
        }
    }

    /// Returns whether there is a queued Command
    pub fn has_command(&self) -> bool {
        self.queued_incoming_commands.len() != 0
    }

    /// Gets the next queued Command
    pub fn pop_command(&mut self) -> Option<(LocalEntityKey, Rc<Box<dyn Event<T>>>)> {
        self.queued_incoming_commands.pop_front()
    }

    /// Queues an Command to be ran locally on the Client
    pub fn queue_command(
        &mut self,
        local_entity_key: LocalEntityKey,
        command: &Rc<Box<dyn Event<T>>>,
    ) {
        let cloned_command = command.clone();
        self.queued_incoming_commands
            .push_front((local_entity_key, cloned_command));
    }
}
