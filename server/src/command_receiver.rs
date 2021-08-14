use std::collections::HashMap;

use naia_shared::{sequence_greater_than, StateType, PawnKey, Manifest, PacketReader, SequenceBuffer, LocalObjectKey, NaiaKey, LocalEntityKey};

const COMMAND_BUFFER_MAX_SIZE: u16 = 64;

/// Handles incoming commands, buffering them to be received on the correct tick
#[derive(Debug)]
pub struct CommandReceiver<T: StateType> {
    queued_incoming_commands: SequenceBuffer<HashMap<PawnKey, T>>,
}

impl<T: StateType> CommandReceiver<T> {
    /// Creates a new CommandReceiver
    pub fn new() -> Self {
        CommandReceiver {
            queued_incoming_commands: SequenceBuffer::with_capacity(COMMAND_BUFFER_MAX_SIZE),
        }
    }

    /// Get the most recently received Command
    pub fn pop_incoming_command(&mut self, server_tick: u16) -> Option<(PawnKey, T)> {
        if let Some(map) = self.queued_incoming_commands.get_mut(server_tick) {
            let mut any_key: Option<PawnKey> = None;
            if let Some(any_key_ref) = map.keys().next() {
                any_key = Some(*any_key_ref);
            }
            if let Some(any_key) = any_key {
                if let Some(command) = map.remove(&any_key) {
                    return Some((any_key, command));
                }
            }
        }
        return None;
    }

    /// Given incoming packet data, read transmitted Command and store them to
    /// be returned to the application
    pub fn process_data(
        &mut self,
        server_tick: u16,
        client_tick: u16,
        reader: &mut PacketReader,
        manifest: &Manifest<T>,
    ) {
        let command_count = reader.read_u8();
        for _x in 0..command_count {

            let is_state = reader.read_u8() == 0;
            let local_key = reader.read_u16();
            let pawn_key = {
                if is_state {
                    PawnKey::State(LocalObjectKey::from_u16(local_key))
                } else {
                    PawnKey::Entity(LocalEntityKey::from_u16(local_key))
                }
            };
            let naia_id: u16 = reader.read_u16();
            let past_commands_number: u8 = reader.read_u8();

            let new_command = manifest.create_state(naia_id, reader);
            if !self.queued_incoming_commands.exists(client_tick) {
                self.queued_incoming_commands
                    .insert(client_tick, HashMap::new());
            }
            if let Some(map) = self.queued_incoming_commands.get_mut(client_tick) {
                map.insert(pawn_key, new_command);
            }

            for _y in 0..past_commands_number {
                let tick_diff = reader.read_u8();
                let past_tick = client_tick.wrapping_sub(tick_diff.into());

                let new_command = manifest.create_state(naia_id, reader);
                if sequence_greater_than(past_tick, server_tick) {
                    if !self.queued_incoming_commands.exists(past_tick) {
                        self.queued_incoming_commands
                            .insert(past_tick, HashMap::new());
                    }
                    if let Some(map) = self.queued_incoming_commands.get_mut(past_tick) {
                        if !map.contains_key(&pawn_key) {
                            map.insert(pawn_key, new_command);
                        }
                    }
                }
            }
        }
    }
}
