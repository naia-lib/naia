use std::rc::Rc;

use naia_shared::{PawnKey, ProtocolType, Replicate};

use super::command_sender::CommandSender;

/// Handles outgoing Commands
#[derive(Debug)]
pub struct DualCommandSender<T: ProtocolType> {
    replicate_manager: CommandSender<T>,
    entity_manager: CommandSender<T>,
}

impl<T: ProtocolType> DualCommandSender<T> {
    /// Creates a new CommandSender
    pub fn new() -> Self {
        DualCommandSender {
            replicate_manager: CommandSender::new(),
            entity_manager: CommandSender::new(),
        }
    }

    /// Gets the next queued Command to be transmitted
    pub fn has_command(&self) -> bool {
        self.replicate_manager.has_command() || self.entity_manager.has_command()
    }

    /// Gets the next queued Command to be transmitted
    pub fn pop_command(&mut self) -> Option<(PawnKey, Rc<Box<dyn Replicate<T>>>)> {
        let replicate_command = self.replicate_manager.pop_command();
        if replicate_command.is_none() {
            return self.entity_manager.pop_command();
        }
        return replicate_command;
    }

    /// If  the last popped Command from the queue somehow wasn't able to be
    /// written into a packet, put the Command back into the front of the queue
    pub fn unpop_command(&mut self, pawn_key: &PawnKey, command: &Rc<Box<dyn Replicate<T>>>) {
        match pawn_key {
            PawnKey::Object(_) => {
                self.replicate_manager.unpop_command(pawn_key, command);
            }
            PawnKey::Entity(_) => {
                self.entity_manager.unpop_command(pawn_key, command);
            }
        }
    }

    /// Queues an Command to be transmitted to the remote host
    pub fn queue_command(&mut self, pawn_key: &PawnKey, command: &impl Replicate<T>) {
        match pawn_key {
            PawnKey::Object(_) => {
                self.replicate_manager.queue_command(pawn_key, command);
            }
            PawnKey::Entity(_) => {
                self.entity_manager.queue_command(pawn_key, command);
            }
        }
    }
}
