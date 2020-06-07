
use std::time::Duration;

use crate::{Timer, PacketType, NetEvent, EventManifest, EntityKey, EntityManager, EntityManifest, PacketWriter, PacketReader, ManagerType, HostType};

use super::{
    sequence_buffer::{SequenceNumber},
    Timestamp,
    ack_manager::AckManager,
    event_manager::EventManager,
    server_entity_manager::ServerEntityManager,
    client_entity_manager::ClientEntityManager,
    EventType,
    EntityType,
    EntityStore
};

pub struct NetConnection<T: EventType, U: EntityType> {
    pub connection_timestamp: Timestamp,
    heartbeat_manager: Timer,
    timeout_manager: Timer,
    ack_manager: AckManager,
    event_manager: EventManager<T>,
    entity_manager: EntityManager<U>,
}

impl<T: EventType, U: EntityType> NetConnection<T, U> {
    pub fn new(host_type: HostType, heartbeat_interval: Duration, timeout_duration: Duration, connection_timestamp: Timestamp) -> Self {

        let entity_manager = match host_type {
            HostType:: Server => EntityManager::Server(ServerEntityManager::new()),
            HostType:: Client => EntityManager::Client(ClientEntityManager::new()),
        };

        return NetConnection {
            connection_timestamp,
            heartbeat_manager: Timer::new(heartbeat_interval),
            timeout_manager: Timer::new(timeout_duration),
            ack_manager: AckManager::new(host_type),
            event_manager: EventManager::new(),
            entity_manager,
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

    pub fn process_incoming(&mut self, payload: &[u8]) -> Box<[u8]> {
        self.ack_manager.process_incoming(&mut self.event_manager, &mut self.entity_manager, payload)
    }

    pub fn process_outgoing(&mut self, packet_type: PacketType, payload: &[u8]) -> Box<[u8]> {
        self.ack_manager.process_outgoing(packet_type, payload)
    }

    pub fn get_next_packet_index(&self) -> SequenceNumber {
        self.ack_manager.local_sequence_num()
    }

    pub fn queue_event(&mut self, event: &impl NetEvent<T>) {
        self.event_manager.queue_outgoing_event(event);
    }

    pub fn get_outgoing_packet(&mut self, manifest: &EventManifest<T>) -> Option<Box<[u8]>> {

        if self.event_manager.has_outgoing_events() {
            let mut writer = PacketWriter::new();

            let next_packet_index = self.get_next_packet_index();
            while let Some(popped_event) = self.event_manager.pop_outgoing_event(next_packet_index) {
                writer.write_event(manifest, &popped_event);
            }

            if writer.has_bytes() {
                // Get bytes from writer
                let out_bytes = writer.get_bytes();

                // Add header to it
                let payload = self.process_outgoing(PacketType::Data, &out_bytes);
                return Some(payload);
            }
        }

        return None;
    }

    pub fn get_incoming_event(&mut self) -> Option<T> {
        return self.event_manager.pop_incoming_event();
    }

    pub fn process_data(&mut self, event_manifest: &EventManifest<T>, entity_manifest: &EntityManifest<U>, data: &mut [u8]) {
        let mut reader = PacketReader::new(data);
        while reader.has_more() {
            match reader.read_manager_type() {
                ManagerType::Event => {
                    self.event_manager.process_data(&mut reader, event_manifest);
                }
                ManagerType::Entity => {
                    //self.server_entity_manager.process_data(&mut reader, entity_manifest);
                }
                _ => {}
            }
        }
    }

    pub fn has_entity(&self, key: EntityKey) -> bool {
        return match &self.entity_manager {
            EntityManager::<U>::Server(entity_manager) => entity_manager.has_entity(key),
            EntityManager::<U>::Client(entity_manager) => entity_manager.has_entity(key),
        }
    }

    pub fn add_entity(&self, key: EntityKey) {
        return match &self.entity_manager {
            EntityManager::<U>::Server(entity_manager) => entity_manager.add_entity(key),
            EntityManager::<U>::Client(entity_manager) => entity_manager.add_entity(key),
        }
    }
}