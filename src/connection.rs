
use std::{
    rc::Rc,
    net::SocketAddr,
};

use crate::{Timer, PacketType, NetEvent, EventManifest, ServerEntityManager,
            EventManager, PacketReader,
            EventType, EntityType};

use super::{
    sequence_buffer::{SequenceNumber},
    Timestamp,
    ack_manager::AckManager,
};

pub struct Connection<T: EventType> {
    address: SocketAddr,
    pub connection_timestamp: Timestamp,
    heartbeat_manager: Timer,
    timeout_manager: Timer,
    ack_manager: AckManager,
    event_manager: EventManager<T>,
}

impl<T: EventType> Connection<T> {
    pub fn new(address: SocketAddr,
               connection_timestamp: Timestamp,
               heartbeat_manager: Timer,
               timeout_manager: Timer,
               ack_manager: AckManager,
               event_manager: EventManager<T>) -> Self {

        return Connection {
            address,
            connection_timestamp,
            heartbeat_manager,
            timeout_manager,
            ack_manager,
            event_manager,
        };
    }

    pub fn mark_sent(&mut self) {
        return self.heartbeat_manager.reset();
    }

    pub fn should_send_heartbeat(&self) -> bool {
        return self.heartbeat_manager.ringing();
    }

    pub fn mark_heard(&mut self) {
        return self.timeout_manager.reset();
    }

    pub fn should_drop(&self) -> bool {
        return self.timeout_manager.ringing();
    }

    pub fn process_incoming_header<U: EntityType>(&mut self, entity_manager: &mut Option<&mut ServerEntityManager<U>>, payload: &[u8]) -> Box<[u8]> {
        return self.ack_manager.process_incoming(&mut self.event_manager, entity_manager, payload);
    }

    pub fn process_outgoing_header(&mut self, packet_type: PacketType, payload: &[u8]) -> Box<[u8]> {
        return self.ack_manager.process_outgoing(packet_type, payload);
    }

    pub fn get_next_packet_index(&self) -> SequenceNumber {
        return self.ack_manager.local_sequence_num();
    }

    pub fn queue_event(&mut self, event: &impl NetEvent<T>) {
        return self.event_manager.queue_outgoing_event(event);
    }

    pub fn has_outgoing_events(&self) -> bool {
        return self.event_manager.has_outgoing_events();
    }

    pub fn pop_outgoing_event(&mut self, next_packet_index: u16) -> Option<Rc<Box<dyn NetEvent<T>>>> {
        return self.event_manager.pop_outgoing_event(next_packet_index);
    }

    pub fn unpop_outgoing_event(&mut self, next_packet_index: u16, event: &Rc<Box<dyn NetEvent<T>>>) {
        return self.event_manager.unpop_outgoing_event(next_packet_index, event);
    }

    pub fn process_event_data(&mut self, reader: &mut PacketReader, manifest: &EventManifest<T>) {
        return self.event_manager.process_data(reader, manifest);
    }


//    pub fn get_outgoing_packet<U: EntityType>(&mut self, event_manifest: &EventManifest<T>, entity_manifest: &EntityManifest<U>) -> Option<Box<[u8]>> {
//
//        let entity_manager_has_outgoing_messages = match &self.entity_manager {
//            EntityManager::Server(server_entity_manager) => server_entity_manager.has_outgoing_messages(),
//            EntityManager::Client(_) => false,
//        };
//        if self.event_manager.has_outgoing_events() || entity_manager_has_outgoing_messages {
//            let mut writer = PacketWriter::new();
//
//            let next_packet_index: u16 = self.get_next_packet_index();
//            while let Some(popped_event) = self.event_manager.pop_outgoing_event(next_packet_index) {
//                if !writer.write_event(event_manifest, &popped_event) {
//                    self.event_manager.unpop_outgoing_event(next_packet_index, &popped_event);
//                    break;
//                }
//            }
//            if let EntityManager::Server(server_entity_manager) = &mut self.entity_manager {
//                while let Some(popped_entity_message) = server_entity_manager.pop_outgoing_message(next_packet_index) {
//                    if !writer.write_entity_message(entity_manifest, &popped_entity_message) {
//                        server_entity_manager.unpop_outgoing_message(next_packet_index, &popped_entity_message);
//                        break;
//                    }
//                }
//            }
//
//            if writer.has_bytes() {
//                // Get bytes from writer
//                let out_bytes = writer.get_bytes();
//
//                // Add header to it
//                let payload = self.process_outgoing_header(PacketType::Data, &out_bytes);
//                return Some(payload);
//            }
//        }
//
//        return None;
//    }

    pub fn get_incoming_event(&mut self) -> Option<T> {
        return self.event_manager.pop_incoming_event();
    }

//    pub fn get_incoming_entity_message<U: EntityType>(&mut self) -> Option<ClientEntityMessage<U>> {
//        if let EntityManager::Client(client_entity_manager) = &mut self.entity_manager {
//            return client_entity_manager.pop_incoming_message();
//        }
//        return None;
//    }

//    pub fn process_incoming_data<U: EntityType>(&mut self, event_manifest: &EventManifest<T>, entity_manifest: &EntityManifest<U>, data: &mut [u8]) {
//        let mut reader = PacketReader::new(data);
//        while reader.has_more() {
//            match reader.read_manager_type() {
//                ManagerType::Event => {
//                    self.event_manager.process_data(&mut reader, event_manifest);
//                }
//                ManagerType::Entity => {
//                    if let EntityManager::Client(client_entity_manager) = &mut self.entity_manager {
//                        client_entity_manager.process_data(&mut reader, entity_manifest);
//                    }
//                }
//                _ => {}
//            }
//        }
//    }
}