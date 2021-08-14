use std::{net::SocketAddr, rc::Rc};

use crate::{wrapping_diff, Timer, EventManager};

use super::{
    ack_manager::AckManager,
    state::{state_notifiable::StateNotifiable, protocol_type::ProtocolType, state::State},
    connection_config::ConnectionConfig,
    manifest::Manifest,
    packet_type::PacketType,
    sequence_buffer::SequenceNumber,
    standard_header::StandardHeader,
    PacketReader,
};

/// Represents a connection to a remote host, and provides functionality to
/// manage the connection and the communications to it
#[derive(Debug)]
pub struct Connection<T: ProtocolType> {
    address: SocketAddr,
    heartbeat_timer: Timer,
    timeout_timer: Timer,
    ack_manager: AckManager,
    event_manager: EventManager<T>,
    last_received_tick: u16,
}

impl<T: ProtocolType> Connection<T> {
    /// Create a new Connection, given the appropriate underlying managers
    pub fn new(address: SocketAddr, config: &ConnectionConfig) -> Self {
        return Connection {
            address,
            heartbeat_timer: Timer::new(config.heartbeat_interval),
            timeout_timer: Timer::new(config.disconnection_timeout_duration),
            ack_manager: AckManager::new(),
            event_manager: EventManager::new(),
            last_received_tick: 0,
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
        header: &StandardHeader,
        state_notifiable: &mut Option<&mut dyn StateNotifiable>,
    ) {
        if wrapping_diff(self.last_received_tick, header.host_tick()) > 0 {
            self.last_received_tick = header.host_tick();
        }
        self.ack_manager
            .process_incoming(&header, &mut self.event_manager, state_notifiable);
    }

    /// Given a packet payload, start tracking the packet via it's index, attach
    /// the appropriate header, and return the packet's resulting underlying
    /// bytes
    pub fn process_outgoing_header(
        &mut self,
        host_tick: u16,
        last_received_tick: u16,
        packet_type: PacketType,
        payload: &[u8],
    ) -> Box<[u8]> {
        // Add header onto message!
        let mut header_bytes = Vec::new();

        let local_packet_index = self.ack_manager.get_local_packet_index();
        let last_remote_packet_index = self.ack_manager.get_last_remote_packet_index();
        let bit_field = self.ack_manager.get_ack_bitfield();

        let header = StandardHeader::new(
            packet_type,
            local_packet_index,
            last_remote_packet_index,
            bit_field,
            host_tick,
            last_received_tick,
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
    pub fn queue_event(&mut self, event: &impl State<T>) {
        return self.event_manager.queue_outgoing_event(event);
    }

    /// Returns whether there are events to be sent to the remote host
    pub fn has_outgoing_events(&self) -> bool {
        return self.event_manager.has_outgoing_events();
    }

    /// Pop the next outgoing event from the queue
    pub fn pop_outgoing_event(&mut self, next_packet_index: u16) -> Option<Rc<Box<dyn State<T>>>> {
        return self.event_manager.pop_outgoing_event(next_packet_index);
    }

    /// If for some reason the next outgoing event could not be written into a
    /// message and sent, place it back into the front of the queue
    pub fn unpop_outgoing_event(&mut self, next_packet_index: u16, event: &Rc<Box<dyn State<T>>>) {
        return self
            .event_manager
            .unpop_outgoing_event(next_packet_index, event);
    }

    /// Given an incoming packet which has been identified as an event, send the
    /// data to the EventManager for processing
    pub fn process_event_data(
        &mut self,
        reader: &mut PacketReader,
        manifest: &Manifest<T>,
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

    /// Get the latest received tick from the remote host
    pub fn get_last_received_tick(&self) -> u16 {
        return self.last_received_tick;
    }
}
