use std::{cell::RefCell, net::SocketAddr, rc::Rc};

use naia_shared::{
    Connection, ConnectionConfig, Entity, EntityType, Event, EventType, ManagerType, Manifest,
    PacketReader, PacketType, PacketWriter, SequenceNumber, StandardHeader,
};

use super::{
    entities::{
        entity_key::entity_key::EntityKey, entity_packet_writer::EntityPacketWriter,
        mut_handler::MutHandler, server_entity_manager::ServerEntityManager,
    },
    ping_manager::PingManager,
};

pub struct ClientConnection<T: EventType, U: EntityType> {
    connection: Connection<T>,
    entity_manager: ServerEntityManager<U>,
    ping_manager: PingManager,
}

impl<T: EventType, U: EntityType> ClientConnection<T, U> {
    pub fn new(
        address: SocketAddr,
        mut_handler: Option<&Rc<RefCell<MutHandler>>>,
        connection_config: &ConnectionConfig,
    ) -> Self {
        ClientConnection {
            connection: Connection::new(address, connection_config),
            entity_manager: ServerEntityManager::new(address, mut_handler.unwrap()),
            ping_manager: PingManager::new(),
        }
    }

    pub fn get_outgoing_packet(&mut self, manifest: &Manifest<T, U>) -> Option<Box<[u8]>> {
        if self.connection.has_outgoing_events() || self.entity_manager.has_outgoing_messages() {
            let mut writer = PacketWriter::new();

            let next_packet_index: u16 = self.get_next_packet_index();
            while let Some(popped_event) = self.connection.pop_outgoing_event(next_packet_index) {
                if !writer.write_event(manifest, &popped_event) {
                    self.connection
                        .unpop_outgoing_event(next_packet_index, &popped_event);
                    break;
                }
            }
            while let Some(popped_entity_message) =
                self.entity_manager.pop_outgoing_message(next_packet_index)
            {
                if !EntityPacketWriter::write_entity_message(
                    &mut writer,
                    manifest,
                    &popped_entity_message,
                ) {
                    self.entity_manager
                        .unpop_outgoing_message(next_packet_index, &popped_entity_message);
                    break;
                }
            }

            if writer.has_bytes() {
                // Get bytes from writer
                let out_bytes = writer.get_bytes();

                // Add header to it
                let payload = self.process_outgoing_header(PacketType::Data, &out_bytes);
                return Some(payload);
            }
        }

        return None;
    }

    pub fn process_incoming_data(&mut self, manifest: &Manifest<T, U>, data: &[u8]) {
        let mut reader = PacketReader::new(data);
        while reader.has_more() {
            let manager_type: ManagerType = reader.read_u8().into();
            match manager_type {
                ManagerType::Event => {
                    self.connection.process_event_data(&mut reader, manifest);
                }
                _ => {}
            }
        }
    }

    pub fn has_entity(&self, key: &EntityKey) -> bool {
        return self.entity_manager.has_entity(key);
    }

    pub fn add_entity(&mut self, key: &EntityKey, entity: &Rc<RefCell<dyn Entity<U>>>) {
        self.entity_manager.add_entity(key, entity);
    }

    pub fn remove_entity(&mut self, key: &EntityKey) {
        self.entity_manager.remove_entity(key);
    }

    pub fn collect_entity_updates(&mut self) {
        self.entity_manager.collect_entity_updates();
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

    pub fn process_incoming_header(&mut self, header: &StandardHeader) {
        self.connection
            .process_incoming_header(header, &mut Some(&mut self.entity_manager));
    }

    pub fn process_outgoing_header(
        &mut self,
        packet_type: PacketType,
        payload: &[u8],
    ) -> Box<[u8]> {
        return self
            .connection
            .process_outgoing_header(packet_type, payload);
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

    pub fn get_address(&self) -> SocketAddr {
        return self.connection.get_address();
    }

    pub fn process_ping(&self, current_tick: u16, ping_payload: &[u8]) -> Box<[u8]> {
        return self.ping_manager.process_ping(current_tick, ping_payload);
    }
}
