use std::{net::SocketAddr, rc::Rc};

use crate::Timer;

use super::{
    ack_manager::AckManager,
    connection_config::ConnectionConfig,
    entities::{entity_notifiable::EntityNotifiable, entity_type::EntityType},
    events::{event::Event, event_manager::EventManager, event_type::EventType},
    manifest::Manifest,
    packet_reader::PacketReader,
    packet_type::PacketType,
    rtt::rtt_tracker::RttTracker,
    sequence_buffer::SequenceNumber,
    shared_config::SharedConfig,
    standard_header::StandardHeader,
    tick_manager::TickManager,
};

/// Represents a connection to a remote host, and provides functionality to
/// manage the connection and the communications to it
#[derive(Debug)]
pub struct Connection<T: EventType> {
    address: SocketAddr,
    heartbeat_timer: Timer,
    timeout_timer: Timer,
    ack_manager: AckManager,
    tick_manager: TickManager,
    rtt_tracker: RttTracker,
    event_manager: EventManager<T>,
}

impl<T: EventType> Connection<T> {
    /// Create a new Connection, given the appropriate underlying managers
    pub fn new(
        address: SocketAddr,
        config: &ConnectionConfig,
        shared_config: &SharedConfig,
    ) -> Self {
        let heartbeat_interval = config.heartbeat_interval;
        let timeout_duration = config.disconnection_timeout_duration;
        let rtt_smoothing_factor = config.rtt_smoothing_factor;
        let rtt_max_value = config.rtt_max_value;

        let tick_interval = shared_config.tick_interval;

        return Connection {
            address,
            heartbeat_timer: Timer::new(heartbeat_interval),
            timeout_timer: Timer::new(timeout_duration),
            ack_manager: AckManager::new(),
            rtt_tracker: RttTracker::new(rtt_smoothing_factor, rtt_max_value),
            event_manager: EventManager::new(),
            tick_manager: TickManager::new(tick_interval),
        };
    }

    /// Record that a message has been sent (to prevent needing to send a
    /// heartbeat)
    pub fn mark_sent(&mut self) {
        return self.heartbeat_timer.reset();
    }

    /// Returns whether a heartbeat message should be sent
    pub fn should_send_heartbeat(&self) -> bool {
        return self.heartbeat_timer.ringing();
    }

    /// Record that a message has been received from a remote host (to prevent
    /// disconnecting from the remote host)
    pub fn mark_heard(&mut self) {
        return self.timeout_timer.reset();
    }

    /// Returns whether this connection should be dropped as a result of a
    /// timeout
    pub fn should_drop(&self) -> bool {
        return self.timeout_timer.ringing();
    }

    /// Process an incoming packet, pulling out the packet index number to keep
    /// track of the current RTT, and sending the packet to the AckManager to
    /// handle packet notification events
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

    /// Given a packet payload, start tracking the packet via it's index, attach
    /// the appropriate header, and return the packet's resulting underlying
    /// bytes
    pub fn process_outgoing_header(
        &mut self,
        packet_type: PacketType,
        payload: &[u8],
    ) -> Box<[u8]> {
        self.rtt_tracker
            .process_outgoing(self.ack_manager.get_local_packet_index());

        // Add header onto message!
        let mut header_bytes = Vec::new();

        let local_packet_index = self.ack_manager.get_local_packet_index();
        let last_remote_packet_index = self.ack_manager.get_last_remote_packet_index();
        let bit_field = self.ack_manager.get_ack_bitfield();
        let current_tick = self.tick_manager.get_current_tick();
        let tick_latency = self.tick_manager.get_tick_latency();

        let header = StandardHeader::new(
            packet_type,
            local_packet_index,
            last_remote_packet_index,
            bit_field,
            current_tick,
            tick_latency,
        );
        header.write(&mut header_bytes);

        // Ack stuff //
        self.ack_manager
            .track_packet(packet_type, local_packet_index);
        self.ack_manager.increment_local_packet_index();
        ///////////////

        [header_bytes.as_slice(), &payload]
            .concat()
            .into_boxed_slice()
    }

    /// Get the next outgoing packet's index
    pub fn get_next_packet_index(&self) -> SequenceNumber {
        return self.ack_manager.get_local_packet_index();
    }

    /// Queue up an event to be sent to the remote host
    pub fn queue_event(&mut self, event: &impl Event<T>) {
        return self.event_manager.queue_outgoing_event(event);
    }

    /// Returns whether there are events to be sent to the remote host
    pub fn has_outgoing_events(&self) -> bool {
        return self.event_manager.has_outgoing_events();
    }

    /// Pop the next outgoing event from the queue
    pub fn pop_outgoing_event(&mut self, next_packet_index: u16) -> Option<Rc<Box<dyn Event<T>>>> {
        return self.event_manager.pop_outgoing_event(next_packet_index);
    }

    /// If for some reason the next outgoing event could not be written into a
    /// message and sent, place it back into the front of the queue
    pub fn unpop_outgoing_event(&mut self, next_packet_index: u16, event: &Rc<Box<dyn Event<T>>>) {
        return self
            .event_manager
            .unpop_outgoing_event(next_packet_index, event);
    }

    /// Given an incoming packet which has been identified as an event, send the
    /// data to the EventManager for processing
    pub fn process_event_data<U: EntityType>(
        &mut self,
        reader: &mut PacketReader,
        manifest: &Manifest<T, U>,
    ) {
        return self.event_manager.process_data(reader, manifest);
    }

    /// Get the most recent event that has been received from a remote host
    pub fn get_incoming_event(&mut self) -> Option<T> {
        return self.event_manager.pop_incoming_event();
    }

    /// Get the address of the remote host
    pub fn get_address(&self) -> SocketAddr {
        return self.address;
    }

    /// Get the Round Trip Time to the remote host
    pub fn get_rtt(&self) -> f32 {
        return self.rtt_tracker.get_rtt();
    }
}
