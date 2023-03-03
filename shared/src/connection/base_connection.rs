use std::{hash::Hash, net::SocketAddr, sync::{Arc, RwLock}};

use naia_serde::{BitReader, BitWriter, Serde, SerdeErr};
use naia_socket_shared::Instant;

use crate::{
    backends::Timer,
    messages::{channels::channel_kinds::ChannelKinds, message_manager::MessageManager},
    types::{HostType, PacketIndex},
    EntityConverter, GlobalDiffHandler, HostLocalWorldManager, MessageKinds, Protocol,
    RemoteWorldManager, Tick, WorldEvents, WorldMutType, WorldRecord, WorldRefType,
};

use super::{
    ack_manager::AckManager, connection_config::ConnectionConfig,
    packet_notifiable::PacketNotifiable, packet_type::PacketType, standard_header::StandardHeader,
};

/// Represents a connection to a remote host, and provides functionality to
/// manage the connection and the communications to it
pub struct BaseConnection<E: Copy + Eq + Hash + Send + Sync> {
    pub address: SocketAddr,
    pub message_manager: MessageManager,
    pub host_world_manager: HostLocalWorldManager<E>,
    pub remote_world_manager: RemoteWorldManager<E>,
    heartbeat_timer: Timer,
    timeout_timer: Timer,
    ack_manager: AckManager,
}

impl<E: Copy + Eq + Hash + Send + Sync> BaseConnection<E> {
    /// Create a new BaseConnection, given the appropriate underlying managers
    pub fn new(
        address: SocketAddr,
        host_type: HostType,
        connection_config: &ConnectionConfig,
        channel_kinds: &ChannelKinds,
        diff_handler: &Arc<RwLock<GlobalDiffHandler<E>>>,
    ) -> Self {
        BaseConnection {
            address,
            heartbeat_timer: Timer::new(connection_config.heartbeat_interval),
            timeout_timer: Timer::new(connection_config.disconnection_timeout_duration),
            ack_manager: AckManager::new(),
            message_manager: MessageManager::new(host_type, channel_kinds),
            host_world_manager: HostLocalWorldManager::new(address, diff_handler),
            remote_world_manager: RemoteWorldManager::new(),
        }
    }

    // Heartbeats

    /// Record that a message has been sent (to prevent needing to send a
    /// heartbeat)
    pub fn mark_sent(&mut self) {
        self.heartbeat_timer.reset()
    }

    /// Returns whether a heartbeat message should be sent
    pub fn should_send_heartbeat(&self) -> bool {
        self.heartbeat_timer.ringing()
    }

    // Timeouts

    /// Record that a message has been received from a remote host (to prevent
    /// disconnecting from the remote host)
    pub fn mark_heard(&mut self) {
        self.timeout_timer.reset()
    }

    /// Returns whether this connection should be dropped as a result of a
    /// timeout
    pub fn should_drop(&self) -> bool {
        self.timeout_timer.ringing()
    }

    // Acks & Headers

    /// Process an incoming packet, pulling out the packet index number to keep
    /// track of the current RTT, and sending the packet to the AckManager to
    /// handle packet notification events
    pub fn process_incoming_header(
        &mut self,
        header: &StandardHeader,
        packet_notifiables: &mut [&mut dyn PacketNotifiable],
    ) {
        self.ack_manager.process_incoming_header(
            header,
            &mut self.message_manager,
            &mut self.host_world_manager,
            packet_notifiables,
        );
    }

    /// Given a packet payload, start tracking the packet via it's index, attach
    /// the appropriate header, and return the packet's resulting underlying
    /// bytes
    pub fn write_outgoing_header(&mut self, packet_type: PacketType, writer: &mut BitWriter) {
        // Add header onto message!
        self.ack_manager
            .next_outgoing_packet_header(packet_type)
            .ser(writer);
    }

    /// Get the next outgoing packet's index
    pub fn next_packet_index(&self) -> PacketIndex {
        self.ack_manager.next_sender_packet_index()
    }

    pub fn has_outgoing_messages(&self) -> bool {
        self.message_manager.has_outgoing_messages()
            || self.host_world_manager.has_outgoing_messages()
    }

    pub fn collect_outgoing_messages(
        &mut self,
        now: &Instant,
        rtt_millis: &f32,
        world_record: &WorldRecord<E>,
        message_kinds: &MessageKinds,
    ) {
        self.host_world_manager.collect_outgoing_messages(
            now,
            rtt_millis,
            world_record,
            message_kinds,
            &mut self.message_manager,
        );
        self.message_manager
            .collect_outgoing_messages(now, rtt_millis);
    }

    fn write_messages(
        &mut self,
        protocol: &Protocol,
        world_record: &WorldRecord<E>,
        writer: &mut BitWriter,
        packet_index: PacketIndex,
        has_written: &mut bool,
    ) {
        let converter = EntityConverter::new(world_record, &self.host_world_manager);
        self.message_manager.write_messages(
            protocol,
            &converter,
            writer,
            packet_index,
            has_written,
        );
    }

    pub fn write_outgoing_packet<W: WorldRefType<E>>(
        &mut self,
        protocol: &Protocol,
        now: &Instant,
        writer: &mut BitWriter,
        packet_index: PacketIndex,
        world: &W,
        world_record: &WorldRecord<E>,
        has_written: &mut bool,
    ) {
        // write messages
        {
            self.write_messages(&protocol, world_record, writer, packet_index, has_written);

            // finish messages
            false.ser(writer);
            writer.release_bits(1);
        }

        // write entity updates
        {
            self.host_world_manager.write_updates(
                &protocol.component_kinds,
                now,
                writer,
                &packet_index,
                world,
                world_record,
                has_written,
            );

            // finish updates
            false.ser(writer);
            writer.release_bits(1);
        }

        // write entity actions
        {
            self.host_world_manager.write_actions(
                &protocol.component_kinds,
                now,
                writer,
                &packet_index,
                world,
                world_record,
                has_written,
            );

            // finish actions
            false.ser(writer);
            writer.release_bits(1);
        }
    }
}
