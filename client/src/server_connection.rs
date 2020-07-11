use std::net::SocketAddr;

use naia_shared::{
    Connection, ConnectionConfig, EntityType, Event, EventType, LocalEntityKey, ManagerType,
    Manifest, PacketReader, PacketType, PacketWriter, SequenceNumber,
};

use super::{
    client_entity_manager::ClientEntityManager, client_entity_message::ClientEntityMessage,
};

#[derive(Debug)]
pub struct ServerConnection<T: EventType, U: EntityType> {
    connection: Connection<T>,
    entity_manager: ClientEntityManager<U>,
}

impl<T: EventType, U: EntityType> ServerConnection<T, U> {
    pub fn new(address: SocketAddr, connection_config: &ConnectionConfig) -> Self {
        return ServerConnection {
            connection: Connection::new(address, connection_config),
            entity_manager: ClientEntityManager::new(),
        };
    }

    pub fn get_outgoing_packet(
        &mut self,
        manifest: &Manifest<T, U>,
        current_tick: u16,
    ) -> Option<Box<[u8]>> {
        if self.connection.has_outgoing_events() {
            let mut writer = PacketWriter::new();

            let next_packet_index: u16 = self.get_next_packet_index();
            while let Some(popped_event) = self.connection.pop_outgoing_event(next_packet_index) {
                if !writer.write_event(manifest, &popped_event) {
                    self.connection
                        .unpop_outgoing_event(next_packet_index, &popped_event);
                    break;
                }
            }

            if writer.has_bytes() {
                // Get bytes from writer
                let out_bytes = writer.get_bytes();

                // Add header to it
                let payload =
                    self.process_outgoing_header(current_tick, PacketType::Data, &out_bytes);
                return Some(payload);
            }
        }

        return None;
    }

    pub fn get_incoming_entity_message(&mut self) -> Option<ClientEntityMessage> {
        return self.entity_manager.pop_incoming_message();
    }

    pub fn process_incoming_data(&mut self, manifest: &Manifest<T, U>, data: &mut [u8]) {
        let mut reader = PacketReader::new(data);
        let start_manager_type: ManagerType = reader.read_u8().into();
        if start_manager_type == ManagerType::Event {
            self.connection.process_event_data(&mut reader, manifest);
        }
        if reader.has_more() {
            self.entity_manager.process_data(&mut reader, manifest);
        }
    }

    pub fn get_local_entity(&self, key: LocalEntityKey) -> Option<&U> {
        return self.entity_manager.get_local_entity(key);
    }

    // Pass-through methods to underlying common connection

    pub fn mark_sent(&mut self) {
        return self.connection.mark_sent();
    }

    pub fn should_send_heartbeat(&self) -> bool {
        return self.connection.should_send_heartbeat();
    }

    pub fn mark_heard(&mut self) {
        return self.connection.mark_heard();
    }

    pub fn should_drop(&self) -> bool {
        return self.connection.should_drop();
    }

    pub fn process_incoming_header(&mut self, payload: &[u8]) -> Box<[u8]> {
        return self.connection.process_incoming_header(payload, &mut None);
    }

    pub fn process_outgoing_header(
        &mut self,
        current_tick: u16,
        packet_type: PacketType,
        payload: &[u8],
    ) -> Box<[u8]> {
        return self
            .connection
            .process_outgoing_header(current_tick, packet_type, payload);
    }

    pub fn get_next_packet_index(&self) -> SequenceNumber {
        return self.connection.get_next_packet_index();
    }

    pub fn queue_event(&mut self, event: &impl Event<T>) {
        return self.connection.queue_event(event);
    }

    pub fn get_incoming_event(&mut self) -> Option<T> {
        return self.connection.get_incoming_event();
    }

    pub fn get_rtt(&self) -> f32 {
        return self.connection.get_rtt();
    }
}
