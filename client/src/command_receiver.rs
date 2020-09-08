use std::{
    collections::{HashMap, VecDeque},
    rc::Rc,
};

use crate::{client_entity_manager::ClientEntityManager, naia_client::LocalEntityKey};
use naia_shared::{wrapping_diff, EntityType, Event, EventType, SequenceBuffer};

const COMMAND_HISTORY_SIZE: u16 = 64;

/// Handles incoming, local, predicted Commands
#[derive(Debug)]
pub struct CommandReceiver<T: EventType> {
    queued_incoming_commands: VecDeque<(u16, LocalEntityKey, Rc<Box<dyn Event<T>>>)>,
    command_history: HashMap<LocalEntityKey, SequenceBuffer<Vec<Rc<Box<dyn Event<T>>>>>>,
    queued_command_replays: VecDeque<(u16, LocalEntityKey, Rc<Box<dyn Event<T>>>)>,
    replay_trigger: HashMap<LocalEntityKey, u16>,
}

impl<T: EventType> CommandReceiver<T> {
    /// Creates a new CommandSender
    pub fn new() -> Self {
        CommandReceiver {
            queued_incoming_commands: VecDeque::new(),
            command_history: HashMap::new(),
            queued_command_replays: VecDeque::new(),
            replay_trigger: HashMap::new(),
        }
    }

    /// Gets the next queued Command
    pub fn pop_command(&mut self) -> Option<(u16, LocalEntityKey, Rc<Box<dyn Event<T>>>)> {
        self.queued_incoming_commands.pop_front()
    }

    /// Gets the next queued Replayed Command
    pub fn pop_command_replay<U: EntityType>(
        &mut self,
        entity_manager: &mut ClientEntityManager<U>,
    ) -> Option<(u16, LocalEntityKey, Rc<Box<dyn Event<T>>>)> {
        for (pawn_key, history_tick) in self.replay_trigger.iter() {
            // set pawn to server authoritative state
            entity_manager.pawn_reset(*pawn_key);

            // clear all
            entity_manager.pawn_clear_history(*pawn_key);

            // trigger replay of historical commands
            if let Some(command_buffer) = self.command_history.get_mut(&pawn_key) {
                self.queued_incoming_commands.clear();
                self.queued_command_replays.clear();

                let current_tick = command_buffer.sequence_num();
                for tick in *history_tick..=current_tick {
                    if let Some(commands) = command_buffer.get_mut(tick) {
                        for command in commands {
                            self.queued_command_replays.push_back((
                                tick,
                                *pawn_key,
                                command.clone(),
                            ));
                        }
                    }
                }
            }
        }

        self.replay_trigger.clear();

        self.queued_command_replays.pop_front()
    }

    /// Queues an Command to be ran locally on the Client
    pub fn queue_command(
        &mut self,
        host_tick: u16,
        local_entity_key: LocalEntityKey,
        command: &Rc<Box<dyn Event<T>>>,
    ) {
        self.queued_incoming_commands
            .push_back((host_tick, local_entity_key, command.clone()));

        if !self.command_history.contains_key(&local_entity_key) {
            self.command_history.insert(
                local_entity_key,
                SequenceBuffer::with_capacity(COMMAND_HISTORY_SIZE),
            );
        }

        if let Some(command_buffer) = self.command_history.get_mut(&local_entity_key) {
            if !command_buffer.exists(host_tick) {
                command_buffer.insert(host_tick, Vec::new());
            }
            if let Some(queue) = command_buffer.get_mut(host_tick) {
                queue.push(command.clone());
            }
        }
    }

    /// Queues commands to be replayed from a given tick
    pub fn replay_commands(&mut self, history_tick: u16, pawn_key: LocalEntityKey) {
        if let Some(tick) = self.replay_trigger.get_mut(&pawn_key) {
            if wrapping_diff(*tick, history_tick) > 0 {
                *tick = history_tick;
            }
        } else {
            self.replay_trigger.insert(pawn_key, history_tick);
        }
    }

    /// Removes command history for a given pawn until a specific tick
    pub fn remove_history_until(&mut self, history_tick: u16, pawn_key: LocalEntityKey) {
        if let Some(command_buffer) = self.command_history.get_mut(&pawn_key) {
            command_buffer.remove_until(history_tick);
        }
    }
}
