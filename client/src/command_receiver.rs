use std::collections::{HashMap, VecDeque};

use naia_shared::{
    wrapping_diff, LocalEntityKey, ProtocolType, Ref, Replicate, SequenceBuffer, SequenceIterator,
};

use super::entity_manager::EntityManager;

const COMMAND_HISTORY_SIZE: u16 = 64;

/// Handles incoming, local, predicted Commands
#[derive(Debug)]
pub struct CommandReceiver<P: ProtocolType> {
    queued_incoming_commands: VecDeque<(u16, LocalEntityKey, Ref<dyn Replicate<P>>)>,
    command_history: HashMap<LocalEntityKey, SequenceBuffer<Ref<dyn Replicate<P>>>>,
    queued_command_replays: VecDeque<(u16, LocalEntityKey, Ref<dyn Replicate<P>>)>,
    replay_trigger: HashMap<LocalEntityKey, u16>,
}

impl<P: ProtocolType> CommandReceiver<P> {
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
    pub fn pop_command(&mut self) -> Option<(u16, LocalEntityKey, Ref<dyn Replicate<P>>)> {
        self.queued_incoming_commands.pop_front()
    }

    /// Gets the next queued Replayed Command
    pub fn pop_command_replay(&mut self) -> Option<(u16, LocalEntityKey, Ref<dyn Replicate<P>>)> {
        self.queued_command_replays.pop_front()
    }

    /// Process any necessary replayed Command
    pub fn process_command_replay(&mut self, entity_manager: &mut EntityManager<P>) {
        for (prediction_key, history_tick) in self.replay_trigger.iter() {
            // set prediction to server authoritative entity
            entity_manager.prediction_reset_entity(prediction_key);

            // trigger replay of historical commands
            if let Some(command_buffer) = self.command_history.get_mut(&prediction_key) {
                self.queued_incoming_commands.clear();
                self.queued_command_replays.clear();

                let current_tick = command_buffer.sequence_num();
                for tick in *history_tick..=current_tick {
                    if let Some(command) = command_buffer.get_mut(tick) {
                        self.queued_command_replays.push_back((
                            tick,
                            *prediction_key,
                            command.clone(),
                        ));
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
        prediction_key: &LocalEntityKey,
        command: &Ref<dyn Replicate<P>>,
    ) {
        self.queued_incoming_commands
            .push_back((host_tick, *prediction_key, command.clone()));

        if let Some(command_buffer) = self.command_history.get_mut(&prediction_key) {
            command_buffer.insert(host_tick, command.clone());
        }
    }

    /// Get number of Commands in the command history for a given Prediction
    pub fn command_history_count(&self, prediction_key: &LocalEntityKey) -> u8 {
        if let Some(command_buffer) = self.command_history.get(&prediction_key) {
            return command_buffer.get_entries_count();
        }
        return 0;
    }

    /// Get an iterator of Commands in the command history for a given
    /// Prediction
    pub fn command_history_iter(
        &self,
        prediction_key: &LocalEntityKey,
        reverse: bool,
    ) -> Option<SequenceIterator<Ref<dyn Replicate<P>>>> {
        if let Some(command_buffer) = self.command_history.get(&prediction_key) {
            return Some(command_buffer.iter(reverse));
        }
        return None;
    }

    /// Queues Commands to be replayed from a given tick
    pub fn replay_commands(&mut self, history_tick: u16, prediction_key: &LocalEntityKey) {
        if let Some(tick) = self.replay_trigger.get_mut(&prediction_key) {
            if wrapping_diff(*tick, history_tick) > 0 {
                *tick = history_tick;
            }
        } else {
            self.replay_trigger.insert(*prediction_key, history_tick);
        }
    }

    /// Removes command history for a given Prediction until a specific tick
    pub fn remove_history_until(&mut self, history_tick: u16, prediction_key: &LocalEntityKey) {
        if let Some(command_buffer) = self.command_history.get_mut(&prediction_key) {
            command_buffer.remove_until(history_tick);
        }
    }

    /// Perform initialization on Prediction creation
    pub fn prediction_init(&mut self, prediction_key: &LocalEntityKey) {
        self.command_history.insert(
            *prediction_key,
            SequenceBuffer::with_capacity(COMMAND_HISTORY_SIZE),
        );
    }

    /// Perform cleanup on Prediction deletion
    pub fn prediction_cleanup(&mut self, prediction_key: &LocalEntityKey) {
        self.command_history.remove(prediction_key);
    }
}
