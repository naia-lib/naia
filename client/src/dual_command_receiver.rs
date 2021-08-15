use std::rc::Rc;

use naia_shared::{Replicate, ProtocolType, SequenceIterator, PawnKey};

use super::{replicate_manager::ReplicateManager, command_receiver::CommandReceiver};

/// Handles incoming, local, predicted Commands
#[derive(Debug)]
pub struct DualCommandReceiver<T: ProtocolType> {
    replicate_manager:  CommandReceiver<T>,
    entity_manager: CommandReceiver<T>,
}

impl<T: ProtocolType> DualCommandReceiver<T> {
    /// Creates a new DualCommandReceiver
    pub fn new() -> Self {
        DualCommandReceiver {
            replicate_manager:  CommandReceiver::new(),
            entity_manager: CommandReceiver::new(),
        }
    }

    /// Gets the next queued Command
    pub fn pop_command(&mut self) -> Option<(u16, PawnKey, Rc<Box<dyn Replicate<T>>>)> {
        let replicate_command = self.replicate_manager.pop_command();
        if replicate_command.is_none() {
            return self.entity_manager.pop_command();
        }
        return replicate_command;
    }

    /// Gets the next queued Replayed Command
    pub fn pop_command_replay<U: ProtocolType>(
        &mut self,
    ) -> Option<(u16, PawnKey, Rc<Box<dyn Replicate<T>>>)> {
        let replicate_command_replay = self.replicate_manager.pop_command_replay::<U>();
        if replicate_command_replay.is_none() {
            return self.entity_manager.pop_command_replay::<U>();
        }
        return replicate_command_replay;
    }

    /// Process any necessary replayed Command
    pub fn process_command_replay<U: ProtocolType>(
        &mut self,
        replicate_manager: &mut ReplicateManager<U>,
    ) {
        self.replicate_manager.process_command_replay::<U>(replicate_manager);
        self.entity_manager.process_command_replay::<U>(replicate_manager);
    }

    /// Queues a Pawn Replicate Command to be ran locally on the Client
    pub fn queue_command(
        &mut self,
        host_tick: u16,
        pawn_key: &PawnKey,
        command: &Rc<Box<dyn Replicate<T>>>,
    ) {
        match pawn_key {
            PawnKey::Object(_) => {
                self.replicate_manager.queue_command(host_tick, pawn_key, command);
            },
            PawnKey::Entity(_) => {
                self.entity_manager.queue_command(host_tick, pawn_key, command);
            }
        }
    }

    /// Get number of Commands in the command history for a given Pawn
    pub fn command_history_count(&self, pawn_key: &PawnKey) -> u8 {
        match pawn_key {
            PawnKey::Object(_) => {
                return self.replicate_manager.command_history_count(pawn_key);
            },
            PawnKey::Entity(_) => {
                return self.entity_manager.command_history_count(pawn_key);
            }
        }
    }

    /// Get an iterator of Commands in the command history for a given Pawn
    pub fn command_history_iter(
        &self,
        pawn_key: &PawnKey,
        reverse: bool,
    ) -> Option<SequenceIterator<Rc<Box<dyn Replicate<T>>>>> {
        match pawn_key {
            PawnKey::Object(_) => {
                return self.replicate_manager.command_history_iter(pawn_key, reverse);
            },
            PawnKey::Entity(_) => {
                return self.entity_manager.command_history_iter(pawn_key, reverse);
            }
        }
    }

    /// Queues Commands to be replayed from a given tick
    pub fn replay_commands(&mut self, history_tick: u16, pawn_key: &PawnKey) {
        match pawn_key {
            PawnKey::Object(_) => {
                return self.replicate_manager.replay_commands(history_tick, pawn_key);
            },
            PawnKey::Entity(_) => {
                return self.entity_manager.replay_commands(history_tick, pawn_key);
            }
        }
    }

    /// Removes command history for a given Pawn until a specific tick
    pub fn remove_history_until(&mut self, history_tick: u16, pawn_key: &PawnKey) {
        match pawn_key {
            PawnKey::Object(_) => {
                return self.replicate_manager.remove_history_until(history_tick, pawn_key);
            },
            PawnKey::Entity(_) => {
                return self.entity_manager.remove_history_until(history_tick, pawn_key);
            }
        }
    }

    /// Perform initialization on Pawn creation
    pub fn pawn_init(&mut self, pawn_key: &PawnKey) {
        match pawn_key {
            PawnKey::Object(_) => {
                return self.replicate_manager.pawn_init(pawn_key);
            },
            PawnKey::Entity(_) => {
                return self.entity_manager.pawn_init(pawn_key);
            }
        }
    }

    /// Perform cleanup on Pawn deletion
    pub fn pawn_cleanup(&mut self, pawn_key: &PawnKey) {
        match pawn_key {
            PawnKey::Object(_) => {
                return self.replicate_manager.pawn_cleanup(pawn_key);
            },
            PawnKey::Entity(_) => {
                return self.entity_manager.pawn_cleanup(pawn_key);
            }
        }
    }
}
