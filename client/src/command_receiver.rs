use std::{collections::VecDeque, rc::Rc};

use crate::{client_entity_manager::ClientEntityManager, naia_client::LocalEntityKey};
use naia_shared::{EntityType, Event, EventType, SequenceBuffer};
use std::collections::HashMap;

const COMMAND_HISTORY_SIZE: u16 = 64;

/// Handles incoming, local, predicted Commands
#[derive(Debug)]
pub struct CommandReceiver<T: EventType> {
    queued_incoming_commands: VecDeque<(LocalEntityKey, Rc<Box<dyn Event<T>>>)>,
    command_history: HashMap<LocalEntityKey, SequenceBuffer<Vec<Rc<Box<dyn Event<T>>>>>>,
    queued_command_replays: VecDeque<(u16, LocalEntityKey, Rc<Box<dyn Event<T>>>)>,
    replay_trigger: Option<(u16, LocalEntityKey)>,
}

impl<T: EventType> CommandReceiver<T> {
    /// Creates a new CommandSender
    pub fn new() -> Self {
        CommandReceiver {
            queued_incoming_commands: VecDeque::new(),
            command_history: HashMap::new(),
            queued_command_replays: VecDeque::new(),
            replay_trigger: None,
        }
    }

    /// Gets the next queued Command
    pub fn pop_command(&mut self) -> Option<(LocalEntityKey, Rc<Box<dyn Event<T>>>)> {
        self.queued_incoming_commands.pop_front()
    }

    /// Gets the next queued Replayed Command
    pub fn pop_command_replay<U: EntityType>(
        &mut self,
        entity_manager: &mut ClientEntityManager<U>,
    ) -> Option<(u16, LocalEntityKey, Rc<Box<dyn Event<T>>>)> {
        if self.replay_trigger.is_some() {
            if let Some((history_tick, pawn_key)) = self.replay_trigger {
                // set pawn to server authoritative state
                entity_manager.pawn_reset(pawn_key);

                // trigger replay of historical commands
                if let Some(command_buffer) = self.command_history.get_mut(&pawn_key) {
                    self.queued_command_replays.clear();

                    let current_tick = command_buffer.sequence_num();
                    for tick in (history_tick + 1)..current_tick {
                        if let Some(commands) = command_buffer.get_mut(tick) {
                            for command in commands {
                                self.queued_command_replays.push_back((
                                    tick,
                                    pawn_key,
                                    command.clone(),
                                ));
                            }
                        }
                    }
                }
            }
            self.replay_trigger = None;
        }

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
            .push_back((local_entity_key, command.clone()));

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
        self.replay_trigger = Some((history_tick, pawn_key));
    }
}
