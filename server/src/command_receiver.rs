use std::collections::HashMap;

use naia_shared::{
    sequence_greater_than, EntityType, EventType, LocalEntityKey, Manifest, PacketReader,
    SequenceBuffer,
};

const COMMAND_BUFFER_MAX_SIZE: u16 = 64;

/// Handles incoming commands, buffering them to be received on the correct tick
#[derive(Debug)]
pub struct CommandReceiver<T: EventType> {
    queued_incoming_commands: SequenceBuffer<HashMap<LocalEntityKey, T>>,
}

impl<T: EventType> CommandReceiver<T> {
    /// Creates a new CommandReceiver
    pub fn new() -> Self {
        CommandReceiver {
            queued_incoming_commands: SequenceBuffer::with_capacity(COMMAND_BUFFER_MAX_SIZE),
        }
    }

    /// Get the most recently received Command
    pub fn pop_incoming_command(&mut self, server_tick: u16) -> Option<(LocalEntityKey, T)> {
        if let Some(map) = self.queued_incoming_commands.get_mut(server_tick) {
            let mut any_key: Option<u16> = None;
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
    pub fn process_data<U: EntityType>(
        &mut self,
        server_tick: u16,
        client_tick: u16,
        reader: &mut PacketReader,
        manifest: &Manifest<T, U>,
    ) {
        let command_count = reader.read_u8();
        for _x in 0..command_count {
            let local_entity_key: LocalEntityKey = reader.read_u16();
            let naia_id: u16 = reader.read_u16();
            let past_commands_number: u8 = reader.read_u8();

            match manifest.create_event(naia_id, reader) {
                Some(new_command) => {
                    if !self.queued_incoming_commands.exists(client_tick) {
                        self.queued_incoming_commands
                            .insert(client_tick, HashMap::new());
                    }
                    if let Some(map) = self.queued_incoming_commands.get_mut(client_tick) {
                        map.insert(local_entity_key, new_command);
                    }
                }
                _ => {}
            }

            for _y in 0..past_commands_number {
                let tick_diff = reader.read_u8();
                let past_tick = client_tick.wrapping_sub(tick_diff.into());

                match manifest.create_event(naia_id, reader) {
                    Some(new_command) => {
                        if sequence_greater_than(past_tick, server_tick) {
                            if !self.queued_incoming_commands.exists(past_tick) {
                                self.queued_incoming_commands
                                    .insert(past_tick, HashMap::new());
                            }
                            if let Some(map) = self.queued_incoming_commands.get_mut(past_tick) {
                                if !map.contains_key(&local_entity_key) {
                                    map.insert(local_entity_key, new_command);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
