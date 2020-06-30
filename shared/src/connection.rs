use std::{net::SocketAddr, rc::Rc};

use crate::Timer;

use super::{
    entities::{entity_notifiable::EntityNotifiable, entity_type::EntityType},
    events::{event::Event, event_manager::EventManager, event_type::EventType},
    manifest::Manifest,
    packet_reader::PacketReader,
    packet_type::PacketType,
    rtt::rtt_tracker::RttTracker,
    standard_header::StandardHeader,
};

use super::{ack_manager::AckManager, sequence_buffer::SequenceNumber};

#[derive(Debug)]
pub struct Connection<T: EventType> {
    address: SocketAddr,
    heartbeat_manager: Timer,
    timeout_manager: Timer,
    ack_manager: AckManager,
    rtt_tracker: RttTracker,
    event_manager: EventManager<T>,
}

impl<T: EventType> Connection<T> {
    pub fn new(
        address: SocketAddr,
        heartbeat_manager: Timer,
        timeout_manager: Timer,
        ack_manager: AckManager,
        rtt_tracker: RttTracker,
        event_manager: EventManager<T>,
    ) -> Self {
        return Connection {
            address,
            heartbeat_manager,
            timeout_manager,
            ack_manager,
            rtt_tracker,
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

    pub fn process_incoming_header(
        &mut self,
        payload: &[u8],
        entity_notifiable: &mut Option<&mut dyn EntityNotifiable>,
    ) -> Box<[u8]> {
        let incoming_sequence_number = StandardHeader::get_sequence(payload);
        self.rtt_tracker.process_incoming(incoming_sequence_number);
        return self.ack_manager.process_incoming(
            payload,
            &mut self.event_manager,
            entity_notifiable,
        );
    }

    pub fn process_outgoing_header(
        &mut self,
        packet_type: PacketType,
        payload: &[u8],
    ) -> Box<[u8]> {
        self.rtt_tracker
            .process_outgoing(self.ack_manager.local_sequence_num());
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
        return self
            .event_manager
            .unpop_outgoing_event(next_packet_index, event);
    }

    pub fn process_event_data<U: EntityType>(
        &mut self,
        reader: &mut PacketReader,
        manifest: &Manifest<T, U>,
    ) {
        return self.event_manager.process_data(reader, manifest);
    }

    pub fn get_incoming_event(&mut self) -> Option<T> {
        return self.event_manager.pop_incoming_event();
    }

    pub fn get_address(&self) -> SocketAddr {
        return self.address;
    }

    pub fn get_rtt(&self) -> f32 {
        return self.rtt_tracker.get_rtt();
    }
}
