use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
};

use naia_shared::{wrapping_diff, ProtocolType, SequenceBuffer, SequenceIterator, WorldMutType};

use super::{entity_manager::EntityManager, owned_entity::OwnedEntity};

const COMMAND_HISTORY_SIZE: u16 = 64;

/// Handles incoming, local, predicted Commands
#[derive(Debug)]
pub struct CommandReceiver<P: ProtocolType, E: Copy + Eq + Hash> {
    queued_incoming_commands: VecDeque<(u16, OwnedEntity<E>, P)>,
    command_history: HashMap<E, SequenceBuffer<P>>,
    queued_command_replays: VecDeque<(u16, OwnedEntity<E>, P)>,
    replay_trigger: HashMap<E, u16>,
}

impl<P: ProtocolType, E: Copy + Eq + Hash> CommandReceiver<P, E> {
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
    pub fn pop_command(&mut self) -> Option<(u16, OwnedEntity<E>, P)> {
        self.queued_incoming_commands.pop_front()
    }

    /// Gets the next queued Replayed Command
    pub fn pop_command_replay(&mut self) -> Option<(u16, OwnedEntity<E>, P)> {
        self.queued_command_replays.pop_front()
    }

    /// Process any necessary replayed Command
    pub fn process_command_replay<W: WorldMutType<P, E>>(
        &mut self,
        world: &mut W,
        entity_manager: &mut EntityManager<P, E>,
    ) {
        for (world_entity, history_tick) in self.replay_trigger.iter() {
            if let Some(predicted_entity) = entity_manager.get_predicted_entity(world_entity) {
                // set prediction to server authoritative entity
                entity_manager.prediction_reset_entity(world, world_entity);

                // trigger replay of historical commands
                if let Some(command_buffer) = self.command_history.get_mut(&world_entity) {
                    // this is suspect .. but I seem to remember it's required to be this
                    // way because we're handling it elsewhere?
                    self.queued_incoming_commands.clear();
                    self.queued_command_replays.clear();

                    // load up the replays
                    let current_tick = command_buffer.sequence_num();
                    for tick in *history_tick..=current_tick {
                        if let Some(command) = command_buffer.get_mut(tick) {
                            self.queued_command_replays.push_back((
                                tick,
                                OwnedEntity::new(world_entity, &predicted_entity),
                                command.clone(),
                            ));
                        }
                    }
                }
            }
        }

        self.replay_trigger.clear();
    }

    /// Queues an Command to be ran locally on the Client
    pub fn send_command(&mut self, host_tick: u16, owned_entity: OwnedEntity<E>, command: P) {
        let world_entity = owned_entity.confirmed;
        self.queued_incoming_commands
            .push_back((host_tick, owned_entity, command.clone()));

        if let Some(command_buffer) = self.command_history.get_mut(&world_entity) {
            command_buffer.insert(host_tick, command);
        }
    }

    /// Get number of Commands in the command history for a given Prediction
    pub fn command_history_count(&self, owned_entity: &E) -> u8 {
        if let Some(command_buffer) = self.command_history.get(owned_entity) {
            return command_buffer.get_entries_count();
        }
        return 0;
    }

    /// Get an iterator of Commands in the command history for a given
    /// Prediction
    pub fn command_history_iter(
        &self,
        owned_entity: &E,
        reverse: bool,
    ) -> Option<SequenceIterator<P>> {
        if let Some(command_buffer) = self.command_history.get(owned_entity) {
            return Some(command_buffer.iter(reverse));
        }
        return None;
    }

    /// Queues Commands to be replayed from a given tick
    pub fn replay_commands(&mut self, history_tick: u16, owned_entity: &E) {
        if let Some(tick) = self.replay_trigger.get_mut(owned_entity) {
            if wrapping_diff(*tick, history_tick) > 0 {
                *tick = history_tick;
            }
        } else {
            self.replay_trigger.insert(*owned_entity, history_tick);
        }
    }

    /// Removes command history for a given Prediction until a specific tick
    pub fn remove_history_until(&mut self, history_tick: u16, owned_entity: &E) {
        if let Some(command_buffer) = self.command_history.get_mut(owned_entity) {
            command_buffer.remove_until(history_tick);
        }
    }

    /// Perform initialization on Prediction creation
    pub fn prediction_init(&mut self, owned_entity: &E) {
        self.command_history.insert(
            *owned_entity,
            SequenceBuffer::with_capacity(COMMAND_HISTORY_SIZE),
        );
    }

    /// Perform cleanup on Prediction deletion
    pub fn prediction_cleanup(&mut self, owned_entity: &E) {
        self.command_history.remove(owned_entity);
    }
}
