use std::net::SocketAddr;

use naia_socket_shared::{PacketReader, Timer};

use crate::{wrapping_diff, MessageManager};

use super::{
    ack_manager::AckManager, connection_config::ConnectionConfig, manifest::Manifest,
    packet_notifiable::PacketNotifiable, packet_type::PacketType, protocol_type::ProtocolType,
    sequence_buffer::SequenceNumber, standard_header::StandardHeader,
};

/// Represents a connection to a remote host, and provides functionality to
/// manage the connection and the communications to it
pub struct Connection<P: ProtocolType> {
    address: SocketAddr,
    heartbeat_timer: Timer,
    timeout_timer: Timer,
    ack_manager: AckManager,
    message_manager: MessageManager<P>,
    last_received_tick: u16,
}

impl<P: ProtocolType> Connection<P> {
    /// Create a new Connection, given the appropriate underlying managers
    pub fn new(address: SocketAddr, config: &ConnectionConfig) -> Self {
        return Connection {
            address,
            heartbeat_timer: Timer::new(config.heartbeat_interval),
            timeout_timer: Timer::new(config.disconnection_timeout_duration),
            ack_manager: AckManager::new(),
            message_manager: MessageManager::new(),
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
        packet_notifiable: &mut Option<&mut dyn PacketNotifiable>,
    ) {
        if wrapping_diff(self.last_received_tick, header.host_tick()) > 0 {
            self.last_received_tick = header.host_tick();
        }
        self.ack_manager
            .process_incoming(&header, &mut self.message_manager, packet_notifiable);
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

    /// Queue up a message to be sent to the remote host
    pub fn queue_message(&mut self, message: P, guaranteed_delivery: bool) {
        return self
            .message_manager
            .queue_outgoing_message(message, guaranteed_delivery);
    }

    /// Returns whether there are messages to be sent to the remote host
    pub fn has_outgoing_messages(&self) -> bool {
        return self.message_manager.has_outgoing_messages();
    }

    /// Pop the next outgoing message from the queue
    pub fn pop_outgoing_message(
        &mut self,
        next_packet_index: u16,
    ) -> Option<P> {
        return self.message_manager.pop_outgoing_message(next_packet_index);
    }

    /// If for some reason the next outgoing message could not be written into a
    /// message and sent, place it back into the front of the queue
    pub fn unpop_outgoing_message(
        &mut self,
        next_packet_index: u16,
        message: P,
    ) {
        return self
            .message_manager
            .unpop_outgoing_message(next_packet_index, message);
    }

    /// Given an incoming packet which has been identified as an message, send
    /// the data to the MessageManager for processing
    pub fn process_message_data(&mut self, reader: &mut PacketReader, manifest: &Manifest<P>, packet_index: u16) {
        return self.message_manager.process_data(reader, manifest, packet_index);
    }

    /// Get the most recent message that has been received from a remote host
    pub fn get_incoming_message(&mut self) -> Option<P> {
        return self.message_manager.pop_incoming_message();
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
