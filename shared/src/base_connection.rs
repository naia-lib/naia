use std::net::SocketAddr;

use naia_socket_shared::{PacketReader, Timer};

use crate::{message_manager::MessageManager, sequence_greater_than};

use super::{
    ack_manager::AckManager, connection_config::ConnectionConfig, manifest::Manifest,
    packet_notifiable::PacketNotifiable, packet_type::PacketType, protocolize::Protocolize,
    replicate::ReplicateSafe, sequence_buffer::SequenceNumber, standard_header::StandardHeader,
};

/// Represents a connection to a remote host, and provides functionality to
/// manage the connection and the communications to it
pub struct BaseConnection<P: Protocolize> {
    address: SocketAddr,
    heartbeat_timer: Timer,
    timeout_timer: Timer,
    ack_manager: AckManager,
    message_manager: MessageManager<P>,
    last_received_tick: u16,
}

impl<P: Protocolize> BaseConnection<P> {
    /// Create a new BaseConnection, given the appropriate underlying managers
    pub fn new(address: SocketAddr, config: &ConnectionConfig) -> Self {
        return BaseConnection {
            address,
            heartbeat_timer: Timer::new(config.heartbeat_interval),
            timeout_timer: Timer::new(config.disconnection_timeout_duration),
            ack_manager: AckManager::new(),
            message_manager: MessageManager::new(),
            last_received_tick: 0,
        };
    }

    /// Get the address of the remote host
    pub fn address(&self) -> SocketAddr {
        return self.address;
    }

    /// Get the latest received tick from the remote host
    pub fn last_received_tick(&self) -> u16 {
        return self.last_received_tick;
    }

    // Heartbeats

    /// Record that a message has been sent (to prevent needing to send a
    /// heartbeat)
    pub fn mark_sent(&mut self) {
        return self.heartbeat_timer.reset();
    }

    /// Returns whether a heartbeat message should be sent
    pub fn should_send_heartbeat(&self) -> bool {
        return self.heartbeat_timer.ringing();
    }

    // Timeouts

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

    // Acks & Headers

    /// Process an incoming packet, pulling out the packet index number to keep
    /// track of the current RTT, and sending the packet to the AckManager to
    /// handle packet notification events
    pub fn process_incoming_header(
        &mut self,
        header: &StandardHeader,
        packet_notifiable: &mut Option<&mut dyn PacketNotifiable>,
    ) {
        if sequence_greater_than(header.host_tick(), self.last_received_tick) {
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
        packet_type: PacketType,
        payload: &[u8],
    ) -> Box<[u8]> {
        // Add header onto message!
        let mut header_bytes = Vec::new();

        let local_packet_index = self.ack_manager.local_packet_index();
        let last_remote_packet_index = self.ack_manager.last_remote_packet_index();
        let bit_field = self.ack_manager.ack_bitfield();

        let header = StandardHeader::new(
            packet_type,
            local_packet_index,
            last_remote_packet_index,
            bit_field,
            host_tick,
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
    pub fn next_packet_index(&self) -> SequenceNumber {
        return self.ack_manager.local_packet_index();
    }

    // Message Manager

    /// Queue up a message to be sent to the remote host
    pub fn send_message<R: ReplicateSafe<P>>(&mut self, message: &R, guaranteed_delivery: bool) {
        return self
            .message_manager
            .queue_outgoing_message(message, guaranteed_delivery);
    }

    /// Write all messages
    pub fn write_messages(&mut self, total_bytes: usize, next_packet_index: u16) {
        return self.message_manager.write_messages(total_bytes, next_packet_index);
    }

    /// Returns whether there are messages to be sent to the remote host
    pub fn has_outgoing_messages(&self) -> bool {
        return self.message_manager.has_outgoing_messages();
    }

    /// Given an incoming packet which has been identified as an message, send
    /// the data to the MessageManager for processing
    pub fn process_message_data(&mut self, reader: &mut PacketReader, manifest: &Manifest<P>) {
        return self.message_manager.process_data(reader, manifest);
    }

    /// Get the most recent message that has been received from a remote host
    pub fn incoming_message(&mut self) -> Option<P> {
        return self.message_manager.pop_incoming_message();
    }

    pub fn writer_has_bytes(&self) -> bool {
        self.message_manager.writer_has_bytes()
    }

    pub fn writer_bytes_number(&self) -> usize {
        return self.message_manager.writer_bytes_number();
    }

    pub fn writer_bytes(&mut self, out_bytes: &mut Vec<u8>) {
        self.message_manager.writer_bytes(out_bytes);
    }
}
