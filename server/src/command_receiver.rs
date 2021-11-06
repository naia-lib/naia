use std::collections::HashMap;

use naia_shared::{
    sequence_greater_than, LocalEntity, Manifest, NaiaKey, PacketReader, ProtocolKindType,
    ProtocolType, SequenceBuffer,
};

const COMMAND_BUFFER_MAX_SIZE: u16 = 64;

/// Handles incoming commands, buffering them to be received on the correct tick
#[derive(Debug)]
pub struct CommandReceiver<P: ProtocolType> {
    queued_incoming_commands: SequenceBuffer<HashMap<LocalEntity, P>>,
}

impl<P: ProtocolType> CommandReceiver<P> {
    /// Creates a new CommandReceiver
    pub fn new() -> Self {
        CommandReceiver {
            queued_incoming_commands: SequenceBuffer::with_capacity(COMMAND_BUFFER_MAX_SIZE),
        }
    }

    /// Get the most recently received Command
    pub fn pop_incoming_command(&mut self, server_tick: u16) -> Option<(LocalEntity, P)> {
        if let Some(map) = self.queued_incoming_commands.get_mut(server_tick) {
            let mut any_entity: Option<LocalEntity> = None;
            if let Some(any_entity_ref) = map.keys().next() {
                any_entity = Some(*any_entity_ref);
            }
            if let Some(any_entity) = any_entity {
                if let Some(command) = map.remove(&any_entity) {
                    return Some((any_entity, command));
                }
            }
        }
        return None;
    }

    /// Given incoming packet data, read transmitted Command and store them to
    /// be returned to the application
    pub fn process_incoming_commands(
        &mut self,
        server_tick_opt: Option<u16>,
        client_tick: u16,
        reader: &mut PacketReader,
        manifest: &Manifest<P>,
    ) {
        let command_count = reader.read_u8();
        for _x in 0..command_count {
            let owned_entity = LocalEntity::from_u16(reader.read_u16());
            let replica_kind: P::Kind = P::Kind::from_u16(reader.read_u16());
            let past_commands_number: u8 = reader.read_u8();

            // process most recent sent command
            let success = self.process_command(
                server_tick_opt,
                client_tick,
                reader,
                manifest,
                owned_entity,
                replica_kind,
            );
            if !success {
                info!("got a late new command");
            }

            // process past commands (for redundancy)
            for _y in 0..past_commands_number {
                let tick_diff = reader.read_u8();
                let past_client_tick = client_tick.wrapping_sub(tick_diff.into());
                self.process_command(
                    server_tick_opt,
                    past_client_tick,
                    reader,
                    manifest,
                    owned_entity,
                    replica_kind,
                );
            }
        }
    }

    fn process_command(
        &mut self,
        server_tick_opt: Option<u16>,
        client_tick: u16,
        reader: &mut PacketReader,
        manifest: &Manifest<P>,
        owned_entity: LocalEntity,
        replica_kind: P::Kind,
    ) -> bool {
        let new_command = manifest.create_replica(replica_kind, reader, 0);

        if let Some(server_tick) = server_tick_opt {
            if sequence_greater_than(client_tick, server_tick) {
                if !self.queued_incoming_commands.exists(client_tick) {
                    self.queued_incoming_commands
                        .insert(client_tick, HashMap::new());
                }
                if let Some(map) = self.queued_incoming_commands.get_mut(client_tick) {
                    if !map.contains_key(&owned_entity) {
                        map.insert(owned_entity, new_command);
                    }
                }

                return true;
            } else {
                return false;
            }
        }

        return false;
    }
}
