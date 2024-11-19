use std::{hash::Hash, net::SocketAddr};

use naia_serde::{BitReader, BitWriter, Serde, SerdeErr};
use naia_socket_shared::Instant;

use crate::{
    backends::Timer,
    messages::{channels::channel_kinds::ChannelKinds, message_manager::MessageManager},
    types::{HostType, PacketIndex},
    world::{
        entity::entity_converters::{EntityConverterMut, GlobalWorldManagerType},
        host::{host_world_manager::HostWorldEvents, host_world_writer::HostWorldWriter},
        local_world_manager::LocalWorldManager,
        remote::remote_world_reader::RemoteWorldReader,
    },
    HostWorldManager, Protocol, RemoteWorldManager, Tick, WorldRefType,
};

use super::{
    ack_manager::AckManager, connection_config::ConnectionConfig,
    packet_notifiable::PacketNotifiable, packet_type::PacketType, standard_header::StandardHeader,
};

/// Represents a connection to a remote host, and provides functionality to
/// manage the connection and the communications to it
pub struct BaseConnection<E: Copy + Eq + Hash + Send + Sync> {
    pub message_manager: MessageManager,
    pub host_world_manager: HostWorldManager<E>,
    pub remote_world_manager: RemoteWorldManager<E>,
    pub remote_world_reader: RemoteWorldReader<E>,
    pub local_world_manager: LocalWorldManager<E>,
    heartbeat_timer: Timer,
    timeout_timer: Timer,
    ack_manager: AckManager,
}

impl<E: Copy + Eq + Hash + Send + Sync> BaseConnection<E> {
    /// Create a new BaseConnection, given the appropriate underlying managers
    pub fn new(
        address: &Option<SocketAddr>,
        host_type: HostType,
        user_key: u64,
        connection_config: &ConnectionConfig,
        channel_kinds: &ChannelKinds,
        global_world_manager: &dyn GlobalWorldManagerType<E>,
    ) -> Self {
        Self {
            heartbeat_timer: Timer::new(connection_config.heartbeat_interval),
            timeout_timer: Timer::new(connection_config.disconnection_timeout_duration),
            ack_manager: AckManager::new(),
            message_manager: MessageManager::new(host_type, channel_kinds),
            host_world_manager: HostWorldManager::new(address, global_world_manager),
            remote_world_manager: RemoteWorldManager::new(),
            remote_world_reader: RemoteWorldReader::new(),
            local_world_manager: LocalWorldManager::new(user_key),
        }
    }

    // Heartbeats

    /// Record that a message has been sent (to prevent needing to send a
    /// heartbeat)
    pub fn mark_sent(&mut self) {
        self.heartbeat_timer.reset();
        self.ack_manager.clear_should_send_empty_ack();
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

    pub fn mark_should_send_empty_ack(&mut self) {
        self.ack_manager.mark_should_send_empty_ack();
    }

    pub fn should_send_empty_ack(&self) -> bool {
        self.ack_manager.should_send_empty_ack()
    }

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
            &mut self.local_world_manager,
            packet_notifiables,
        );
    }

    /// Given a packet payload, start tracking the packet via it's index, attach
    /// the appropriate header, and return the packet's resulting underlying
    /// bytes
    pub fn write_header(&mut self, packet_type: PacketType, writer: &mut BitWriter) {
        // Add header onto message!
        self.ack_manager
            .next_outgoing_packet_header(packet_type)
            .ser(writer);
    }

    /// Get the next outgoing packet's index
    pub fn next_packet_index(&self) -> PacketIndex {
        self.ack_manager.next_sender_packet_index()
    }

    pub fn collect_messages(&mut self, now: &Instant, rtt_millis: &f32) {
        self.host_world_manager
            .handle_dropped_packets(now, rtt_millis);
        self.message_manager
            .collect_outgoing_messages(now, rtt_millis);
    }

    fn write_messages(
        &mut self,
        protocol: &Protocol,
        global_world_manager: &dyn GlobalWorldManagerType<E>,
        writer: &mut BitWriter,
        packet_index: PacketIndex,
        has_written: &mut bool,
    ) {
        let mut converter =
            EntityConverterMut::new(global_world_manager, &mut self.local_world_manager);
        self.message_manager.write_messages(
            protocol,
            &mut converter,
            writer,
            packet_index,
            has_written,
        );
    }

    pub fn write_packet<W: WorldRefType<E>>(
        &mut self,
        protocol: &Protocol,
        now: &Instant,
        writer: &mut BitWriter,
        packet_index: PacketIndex,
        world: &W,
        global_world_manager: &dyn GlobalWorldManagerType<E>,
        has_written: &mut bool,
        write_world_events: bool,
        host_world_events: &mut HostWorldEvents<E>,
    ) {
        // write messages
        self.write_messages(
            &protocol,
            global_world_manager,
            writer,
            packet_index,
            has_written,
        );

        // write world events
        if write_world_events {
            HostWorldWriter::write_into_packet(
                &protocol.component_kinds,
                now,
                writer,
                &packet_index,
                world,
                global_world_manager,
                &mut self.local_world_manager,
                has_written,
                &mut self.host_world_manager,
                host_world_events,
            );
        }
    }

    pub fn read_packet(
        &mut self,
        protocol: &Protocol,
        client_tick: &Tick,
        global_world_manager: &dyn GlobalWorldManagerType<E>,
        read_world_events: bool,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        // read messages
        self.message_manager.read_messages(
            protocol,
            &mut self.remote_world_manager.entity_waitlist,
            global_world_manager.to_global_entity_converter(),
            &self.local_world_manager,
            reader,
        )?;

        // read world events
        if read_world_events {
            self.remote_world_reader.read_world_events(
                global_world_manager,
                &mut self.local_world_manager,
                protocol,
                client_tick,
                reader,
            )?;
        }

        Ok(())
    }

    pub fn remote_entities(&self) -> Vec<E> {
        self.local_world_manager.remote_entities()
    }
}
