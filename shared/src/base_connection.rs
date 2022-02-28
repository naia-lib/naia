use std::net::SocketAddr;

use naia_serde::{BitWriter, Serde};
use naia_socket_shared::Timer;

use super::{
    ack_manager::AckManager,
    connection_config::ConnectionConfig,
    message_manager::MessageManager,
    packet_notifiable::PacketNotifiable,
    packet_type::PacketType,
    protocolize::Protocolize,
    standard_header::StandardHeader,
    types::{PacketIndex, Tick},
    wrapping_number::sequence_greater_than,
};

/// Represents a connection to a remote host, and provides functionality to
/// manage the connection and the communications to it
pub struct BaseConnection<P: Protocolize> {
    pub address: SocketAddr,
    heartbeat_timer: Timer,
    timeout_timer: Timer,
    ack_manager: AckManager,
    pub message_manager: MessageManager<P>,
    pub last_received_tick: u16,
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
        self.ack_manager.process_incoming_header(
            &header,
            &mut self.message_manager,
            packet_notifiable,
        );
    }

    /// Given a packet payload, start tracking the packet via it's index, attach
    /// the appropriate header, and return the packet's resulting underlying
    /// bytes
    pub fn write_outgoing_header(
        &mut self,
        host_tick: Tick,
        packet_type: PacketType,
        writer: &mut BitWriter,
    ) {
        // Add header onto message!
        self.ack_manager
            .next_outgoing_packet_header(host_tick, packet_type)
            .ser(writer);
    }

    /// Get the next outgoing packet's index
    pub fn next_packet_index(&self) -> PacketIndex {
        return self.ack_manager.next_sender_packet_index();
    }
}
