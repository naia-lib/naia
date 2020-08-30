use std::{collections::VecDeque, rc::Rc};

use naia_shared::{Event, EventClone, EventType};

/// Handles outgoing Commands
#[derive(Debug)]
pub struct CommandSender<T: EventType> {
    queued_outgoing_commands: VecDeque<Rc<Box<dyn Event<T>>>>,
}

impl<T: EventType> CommandSender<T> {
    /// Creates a new CommandSender
    pub fn new() -> Self {
        CommandSender {
            queued_outgoing_commands: VecDeque::new(),
        }
    }

    /// Gets the next queued Command to be transmitted
    pub fn has_command(&self) -> bool {
        self.queued_outgoing_commands.len() != 0
    }

    /// Gets the next queued Command to be transmitted
    pub fn pop_command(&mut self) -> Option<Rc<Box<dyn Event<T>>>> {
        self.queued_outgoing_commands.pop_front()
    }

    /// If  the last popped Command from the queue somehow wasn't able to be
    /// written into a packet, put the Command back into the front of the queue
    pub fn unpop_command(&mut self, command: &Rc<Box<dyn Event<T>>>) {
        let cloned_command = command.clone();
        self.queued_outgoing_commands.push_front(cloned_command);
    }

    /// Queues an Command to be transmitted to the remote host
    pub fn queue_command(&mut self, command: &impl Event<T>) {
        let clone = Rc::new(EventClone::clone_box(command));
        self.queued_outgoing_commands.push_back(clone);
    }
}
