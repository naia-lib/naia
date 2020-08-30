use std::{collections::VecDeque, rc::Rc};

use naia_shared::{Event, EventClone, EventType};

/// Handles outgoing Commands
#[derive(Debug)]
pub struct CommandSender<T: EventType> {
    queued_outgoing_events: VecDeque<Rc<Box<dyn Event<T>>>>,
}

impl<T: EventType> CommandSender<T> {
    /// Creates a new CommandSender
    pub fn new() -> Self {
        CommandSender {
            queued_outgoing_events: VecDeque::new(),
        }
    }

    /// Gets the next queued Command to be transmitted
    pub fn has_command(&self) -> bool {
        self.queued_outgoing_events.len() != 0
    }

    /// Gets the next queued Command to be transmitted
    pub fn pop_command(&mut self) -> Option<Rc<Box<dyn Event<T>>>> {
        self.queued_outgoing_events.pop_front()
    }

    /// If  the last popped Command from the queue somehow wasn't able to be
    /// written into a packet, put the Command back into the front of the queue
    pub fn unpop_command(&mut self, event: &Rc<Box<dyn Event<T>>>) {
        let cloned_event = event.clone();
        self.queued_outgoing_events.push_front(cloned_event);
    }

    /// Queues an Command to be transmitted to the remote host
    pub fn queue_command(&mut self, event: &impl Event<T>) {
        let clone = Rc::new(EventClone::clone_box(event));
        self.queued_outgoing_events.push_back(clone);
    }
}
