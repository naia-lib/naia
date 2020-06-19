
use std::{
    rc::Rc,
    net::SocketAddr,
};

use crate::{Timer, PacketType, Event, Manifest, EventManager, PacketReader, EventType, EntityNotifiable, EntityType};

use super::{
    sequence_buffer::{SequenceNumber},
    Timestamp,
    ack_manager::AckManager,
};

pub struct Connection<T: EventType> {
    address: SocketAddr,
    heartbeat_manager: Timer,
    timeout_manager: Timer,
    ack_manager: AckManager,
    event_manager: EventManager<T>,
}

impl<T: EventType> Connection<T> {
    pub fn new(address: SocketAddr,
               heartbeat_manager: Timer,
               timeout_manager: Timer,
               ack_manager: AckManager,
               event_manager: EventManager<T>) -> Self {

        return Connection {
            address,
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

    pub fn process_incoming_header(&mut self,
                                   payload: &[u8],
                                   entity_notifiable: &mut Option<&mut dyn EntityNotifiable>) -> Box<[u8]> {
        return self.ack_manager.process_incoming(payload,
                                                 &mut self.event_manager,
                                                 entity_notifiable);
    }

    pub fn process_outgoing_header(&mut self, packet_type: PacketType, payload: &[u8]) -> Box<[u8]> {
        return self.ack_manager.process_outgoing(packet_type, payload);
    }

    pub fn get_next_packet_index(&self) -> SequenceNumber {
        return self.ack_manager.local_sequence_num();
    }

    pub fn queue_event(&mut self, event: &impl Event<T>) {
        return self.event_manager.queue_outgoing_event(event);
    }

    pub fn has_outgoing_events(&self) -> bool {
        return self.event_manager.has_outgoing_events();
    }

    pub fn pop_outgoing_event(&mut self, next_packet_index: u16) -> Option<Rc<Box<dyn Event<T>>>> {
        return self.event_manager.pop_outgoing_event(next_packet_index);
    }

    pub fn unpop_outgoing_event(&mut self, next_packet_index: u16, event: &Rc<Box<dyn Event<T>>>) {
        return self.event_manager.unpop_outgoing_event(next_packet_index, event);
    }

    pub fn process_event_data<U: EntityType>(&mut self, reader: &mut PacketReader, manifest: &Manifest<T, U>) {
        return self.event_manager.process_data(reader, manifest);
    }

    pub fn get_incoming_event(&mut self) -> Option<T> {
        return self.event_manager.pop_incoming_event();
    }
}