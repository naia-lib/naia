use std::{
    collections::{HashMap, VecDeque},
    rc::Rc,
};

use naia_shared::{wrapping_diff, ActorType, Event, EventType, SequenceBuffer, SequenceIterator, LocalEntityKey, LocalActorKey};

use super::client_actor_manager::ClientActorManager;

const COMMAND_HISTORY_SIZE: u16 = 64;

/// Handles incoming, local, predicted Commands
#[derive(Debug)]
pub struct CommandReceiver<T: EventType> {
    queued_incoming_commands: VecDeque<(u16, LocalActorKey, Rc<Box<dyn Event<T>>>)>,
    command_history: HashMap<LocalActorKey, SequenceBuffer<Rc<Box<dyn Event<T>>>>>,
    entity_command_history: HashMap<LocalActorKey, SequenceBuffer<Rc<Box<dyn Event<T>>>>>,
    queued_command_replays: VecDeque<(u16, LocalActorKey, Rc<Box<dyn Event<T>>>)>,
    replay_trigger: HashMap<LocalActorKey, u16>,
}

impl<T: EventType> CommandReceiver<T> {
    /// Creates a new CommandSender
    pub fn new() -> Self {
        CommandReceiver {
            queued_incoming_commands: VecDeque::new(),
            command_history: HashMap::new(),
            entity_command_history: HashMap::new(),
            queued_command_replays: VecDeque::new(),
            replay_trigger: HashMap::new(),
        }
    }

    /// Gets the next queued Command
    pub fn pop_command(&mut self) -> Option<(u16, LocalActorKey, Rc<Box<dyn Event<T>>>)> {
        self.queued_incoming_commands.pop_front()
    }

    /// Gets the next queued Replayed Command
    pub fn pop_command_replay<U: ActorType>(
        &mut self,
    ) -> Option<(u16, LocalActorKey, Rc<Box<dyn Event<T>>>)> {
        self.queued_command_replays.pop_front()
    }

    /// Process any necessary replayed Command
    pub fn process_command_replay<U: ActorType>(
        &mut self,
        actor_manager: &mut ClientActorManager<U>,
    ) {
        for (pawn_key, history_tick) in self.replay_trigger.iter() {
            // set pawn to server authoritative state
            actor_manager.pawn_reset(pawn_key);

            // trigger replay of historical commands
            if let Some(command_buffer) = self.command_history.get_mut(&pawn_key) {
                self.queued_incoming_commands.clear();
                self.queued_command_replays.clear();

                let current_tick = command_buffer.sequence_num();
                for tick in *history_tick..=current_tick {
                    if let Some(command) = command_buffer.get_mut(tick) {
                        self.queued_command_replays
                            .push_back((tick, *pawn_key, command.clone()));
                    }
                }
            }
        }

        self.replay_trigger.clear();
    }

    /// Queues an Command to be ran locally on the Client
    pub fn queue_command(
        &mut self,
        host_tick: u16,
        pawn_key: LocalActorKey,
        command: &Rc<Box<dyn Event<T>>>,
    ) {
        self.queued_incoming_commands
            .push_back((host_tick, pawn_key, command.clone()));

        if let Some(command_buffer) = self.command_history.get_mut(&pawn_key) {
            command_buffer.insert(host_tick, command.clone());
        }
    }

    /// Get number of Commands in the command history for a given Pawn
    pub fn command_history_count(&self, pawn_key: LocalActorKey) -> u8 {
        if let Some(command_buffer) = self.command_history.get(&pawn_key) {
            return command_buffer.get_entries_count();
        }
        return 0;
    }

    /// Get an iterator of Commands in the command history for a given Pawn
    pub fn command_history_iter(
        &self,
        pawn_key: LocalActorKey,
        reverse: bool,
    ) -> Option<SequenceIterator<Rc<Box<dyn Event<T>>>>> {
        if let Some(command_buffer) = self.command_history.get(&pawn_key) {
            return Some(command_buffer.iter(reverse));
        }
        return None;
    }

    /// Queues commands to be replayed from a given tick
    pub fn replay_commands(&mut self, history_tick: u16, pawn_key: LocalActorKey) {
        if let Some(tick) = self.replay_trigger.get_mut(&pawn_key) {
            if wrapping_diff(*tick, history_tick) > 0 {
                *tick = history_tick;
            }
        } else {
            self.replay_trigger.insert(pawn_key, history_tick);
        }
    }

    /// Removes command history for a given pawn until a specific tick
    pub fn remove_history_until(&mut self, history_tick: u16, pawn_key: LocalActorKey) {
        if let Some(command_buffer) = self.command_history.get_mut(&pawn_key) {
            command_buffer.remove_until(history_tick);
        }
    }

    /// Perform initialization on pawn creation
    pub fn pawn_init(&mut self, pawn_key: &LocalActorKey) {
        self.command_history.insert(
            *pawn_key,
            SequenceBuffer::with_capacity(COMMAND_HISTORY_SIZE),
        );
    }

    /// Perform cleanup on pawn deletion
    pub fn pawn_cleanup(&mut self, pawn_key: &LocalActorKey) {
        self.command_history.remove(pawn_key);
    }

    /// Perform initialization on pawn entity creation
    pub fn pawn_entity_init(&mut self, pawn_key: &LocalEntityKey) {
        self.entity_command_history.insert(
            *pawn_key,
            SequenceBuffer::with_capacity(COMMAND_HISTORY_SIZE),
        );
    }

    /// Perform cleanup on pawn entity deletion
    pub fn pawn_entity_cleanup(&mut self, pawn_key: &LocalEntityKey) {
        self.entity_command_history.remove(pawn_key);
    }
}
