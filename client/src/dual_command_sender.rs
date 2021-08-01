use std::rc::Rc;

use naia_shared::{Event, EventType, PawnKey};

use super::command_sender::CommandSender;

/// Handles outgoing Commands
#[derive(Debug)]
pub struct DualCommandSender<T: EventType> {
    state_manager:  CommandSender<T>,
    entity_manager: CommandSender<T>,
}

impl<T: EventType> DualCommandSender<T> {
    /// Creates a new CommandSender
    pub fn new() -> Self {
        DualCommandSender {
            state_manager:  CommandSender::new(),
            entity_manager: CommandSender::new(),
        }
    }

    /// Gets the next queued Command to be transmitted
    pub fn has_command(&self) -> bool {
        self.state_manager.has_command() || self.entity_manager.has_command()
    }

    /// Gets the next queued Command to be transmitted
    pub fn pop_command(&mut self) -> Option<(PawnKey, Rc<Box<dyn Event<T>>>)> {
        let state_command = self.state_manager.pop_command();
        if state_command.is_none() {
            return self.entity_manager.pop_command();
        }
        return state_command;
    }

    /// If  the last popped Command from the queue somehow wasn't able to be
    /// written into a packet, put the Command back into the front of the queue
    pub fn unpop_command(&mut self, pawn_key: &PawnKey, command: &Rc<Box<dyn Event<T>>>) {
        match pawn_key {
            PawnKey::State(_) => {
                self.state_manager.unpop_command(pawn_key, command);
            },
            PawnKey::Entity(_) => {
                self.entity_manager.unpop_command(pawn_key, command);
            }
        }
    }

    /// Queues an Command to be transmitted to the remote host
    pub fn queue_command(&mut self, pawn_key: &PawnKey, command: &impl Event<T>) {
        match pawn_key {
            PawnKey::State(_) => {
                self.state_manager.queue_command(pawn_key, command);
            },
            PawnKey::Entity(_) => {
                self.entity_manager.queue_command(pawn_key, command);
            }
        }
    }
}
