
use std::{
    time::Duration,
    net::SocketAddr,
};

use gaia_shared::{Timer, PacketType, NetEvent, EventManifest, ClientEntityManager,
            EventManager, EntityManager, EntityManifest, PacketWriter, PacketReader, ManagerType, HostType,
            EventType, EntityType, ClientEntityMessage, LocalEntityKey, AckManager, Timestamp, SequenceNumber};

pub struct ClientConnection<T: EventType, U: EntityType> {
    pub connection_timestamp: Timestamp,
    address: SocketAddr,
    heartbeat_manager: Timer,
    timeout_manager: Timer,
    ack_manager: AckManager,
    event_manager: EventManager<T>,
    entity_manager: ClientEntityManager<U>,
}

impl<T: EventType, U: EntityType> ClientConnection<T, U> {
    pub fn new(address: SocketAddr, heartbeat_interval: Duration, timeout_duration: Duration, connection_timestamp: Timestamp) -> Self {

        return ClientConnection {
            address,
            connection_timestamp,
            heartbeat_manager: Timer::new(heartbeat_interval),
            timeout_manager: Timer::new(timeout_duration),
            ack_manager: AckManager::new(HostType::Client),
            event_manager: EventManager::new(),
            entity_manager: ClientEntityManager::new(),
        };
    }

    pub fn mark_sent(&mut self) {
        self.heartbeat_manager.reset();
    }

    pub fn should_send_heartbeat(&self) -> bool {
        self.heartbeat_manager.ringing()
    }

    pub fn mark_heard(&mut self) {
        self.timeout_manager.reset();
    }

    pub fn should_drop(&self) -> bool {
        self.timeout_manager.ringing()
    }

    pub fn process_incoming_header(&mut self, payload: &[u8]) -> Box<[u8]> {
        self.ack_manager.process_incoming(&mut self.event_manager, &mut Option::<&mut EntityManager<U>>::None, payload)
    }

    pub fn process_outgoing_header(&mut self, packet_type: PacketType, payload: &[u8]) -> Box<[u8]> {
        self.ack_manager.process_outgoing(packet_type, payload)
    }

    pub fn get_next_packet_index(&self) -> SequenceNumber {
        self.ack_manager.local_sequence_num()
    }

    pub fn queue_event(&mut self, event: &impl NetEvent<T>) {
        self.event_manager.queue_outgoing_event(event);
    }

    pub fn get_outgoing_packet(&mut self, event_manifest: &EventManifest<T>) -> Option<Box<[u8]>> {

        if self.event_manager.has_outgoing_events() {
            let mut writer = PacketWriter::new();

            let next_packet_index: u16 = self.get_next_packet_index();
            while let Some(popped_event) = self.event_manager.pop_outgoing_event(next_packet_index) {
                if !writer.write_event(event_manifest, &popped_event) {
                    self.event_manager.unpop_outgoing_event(next_packet_index, &popped_event);
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

    pub fn get_incoming_event(&mut self) -> Option<T> {
        return self.event_manager.pop_incoming_event();
    }

    pub fn get_incoming_entity_message(&mut self) -> Option<ClientEntityMessage<U>> {
        return self.entity_manager.pop_incoming_message();
    }

    pub fn process_incoming_data(&mut self, event_manifest: &EventManifest<T>, entity_manifest: &EntityManifest<U>, data: &mut [u8]) {
        let mut reader = PacketReader::new(data);
        while reader.has_more() {
            match reader.read_manager_type() {
                ManagerType::Event => {
                    self.event_manager.process_data(&mut reader, event_manifest);
                }
                ManagerType::Entity => {
                    self.entity_manager.process_data(&mut reader, entity_manifest);
                }
                _ => {}
            }
        }
    }

    pub fn get_local_entity(&self, key: LocalEntityKey) -> Option<&U> {
        return self.entity_manager.get_local_entity(key);
    }
}