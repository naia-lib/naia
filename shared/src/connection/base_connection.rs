use std::{
    collections::{HashMap, HashSet, VecDeque},
    hash::Hash,
    net::SocketAddr,
};

use naia_serde::{BitReader, BitWriter, Serde, SerdeErr};
use naia_socket_shared::Instant;

use crate::connection::bandwidth_accumulator::BandwidthAccumulator;
use crate::world::local::local_world_manager::LocalWorldManager;
use crate::world::world_reader::WorldReader;
use crate::world::world_writer::WorldWriter;
use crate::{
    messages::{channels::channel_kinds::ChannelKinds, message_manager::MessageManager},
    types::{HostType, PacketIndex},
    world::{
        entity::entity_converters::GlobalWorldManagerType, host::host_world_manager::CommandId,
    },
    AckManager, ComponentKind, ComponentKinds, ConnectionConfig, EntityAndGlobalEntityConverter,
    EntityCommand, GlobalEntity, MessageKinds, PacketNotifiable, PacketType, StandardHeader, Tick,
    Timer, WorldRefType,
};

/// Represents a connection to a remote host, and provides functionality to
/// manage the connection and the communications to it
pub struct BaseConnection {
    pub message_manager: MessageManager,
    pub world_manager: LocalWorldManager,
    ack_manager: AckManager,
    heartbeat_timer: Timer,
    bandwidth_accumulator: BandwidthAccumulator,
}

impl BaseConnection {
    /// Create a new BaseConnection, given the appropriate underlying managers
    pub fn new(
        connection_config: &ConnectionConfig,
        address: &Option<SocketAddr>,
        host_type: HostType,
        user_key: u64,
        channel_kinds: &ChannelKinds,
        global_world_manager: &dyn GlobalWorldManagerType,
    ) -> Self {
        Self {
            message_manager: MessageManager::new(host_type, channel_kinds),
            world_manager: LocalWorldManager::new(
                address,
                host_type,
                user_key,
                global_world_manager,
            ),
            ack_manager: AckManager::new(),
            heartbeat_timer: Timer::new(connection_config.heartbeat_interval),
            bandwidth_accumulator: BandwidthAccumulator::new(&connection_config.bandwidth),
        }
    }

    // Bandwidth accumulator (outbound token-bucket cap)

    /// Tick the bandwidth accumulator, adding `target_bytes_per_sec × dt` to
    /// the budget and refreshing the one-packet-overshoot allowance.
    pub fn accumulate_bandwidth(&mut self, now: &Instant) {
        self.bandwidth_accumulator.accumulate(now);
    }

    /// Check whether a packet of `estimated_bytes` is permitted under the
    /// current budget. Allows one MTU-sized overshoot per tick when the
    /// budget is positive but short.
    pub fn can_spend_bandwidth(&self, estimated_bytes: u32) -> bool {
        self.bandwidth_accumulator.can_spend(estimated_bytes)
    }

    /// Subtract `actual_bytes` from the bandwidth budget after a send.
    pub fn spend_bandwidth(&mut self, actual_bytes: u32) {
        self.bandwidth_accumulator.spend(actual_bytes);
    }

    /// Current remaining budget (may be negative after overshoot).
    pub fn bandwidth_remaining(&self) -> f64 {
        self.bandwidth_accumulator.remaining()
    }

    /// Bytes sent during the most-recently-completed send cycle (D13 telemetry).
    pub fn bandwidth_bytes_sent_last_tick(&self) -> u64 {
        self.bandwidth_accumulator.bytes_sent_last_tick()
    }

    /// Packets deferred by the budget gate during the most-recently-completed
    /// send cycle. Always 0 unless `bench_instrumentation` is enabled.
    pub fn bandwidth_packets_deferred_last_tick(&self) -> u32 {
        self.bandwidth_accumulator.packets_deferred_last_tick()
    }

    /// Record that a packet was deferred by the budget gate this cycle.
    /// Invoked from send loops when `can_spend_bandwidth` returns false.
    pub fn record_bandwidth_deferred(&mut self) {
        self.bandwidth_accumulator.record_deferred();
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

    // Acks & Headers

    pub fn mark_should_send_empty_ack(&mut self) {
        self.ack_manager.mark_should_send_empty_ack();
    }

    pub fn should_send_empty_ack(&self) -> bool {
        self.ack_manager.should_send_empty_ack()
    }

    pub fn take_should_send_empty_ack(&mut self) -> bool {
        self.ack_manager.take_should_send_empty_ack()
    }

    /// Process an incoming packet, pulling out the packet index number to keep
    /// track of the current RTT, and sending the packet to the AckManager to
    /// handle packet notification events
    pub fn process_incoming_header(
        &mut self,
        header: &StandardHeader,
        packet_notifiables: &mut [&mut dyn PacketNotifiable],
    ) {
        let mut base_packet_notifiables: [&mut dyn PacketNotifiable; 2] =
            [&mut self.message_manager, &mut self.world_manager];
        self.ack_manager.process_incoming_header(
            header,
            &mut base_packet_notifiables,
            packet_notifiables,
        );
    }

    /// Given a packet payload, start tracking the packet via it's index, attach
    /// the appropriate header, and return the packet's resulting underlying
    /// bytes
    pub fn write_header(
        &mut self,
        packet_type: PacketType,
        writer: &mut BitWriter,
    ) -> StandardHeader {
        let header = self.ack_manager.next_outgoing_packet_header(packet_type);
        header.ser(writer);
        header
    }

    /// Get the next outgoing packet's index
    pub fn next_packet_index(&self) -> PacketIndex {
        self.ack_manager.next_sender_packet_index()
    }

    pub fn last_received_packet_index(&self) -> PacketIndex {
        self.ack_manager.last_received_packet_index()
    }

    /// Fraction of sent data-packets that were lost in the last 64-packet window.
    pub fn packet_loss_pct(&self) -> f32 {
        self.ack_manager.packet_loss_pct()
    }

    pub fn collect_messages(&mut self, now: &Instant, rtt_millis: &f32) {
        self.world_manager.collect_messages(now, rtt_millis);
        self.message_manager
            .collect_outgoing_messages(now, rtt_millis);
    }

    fn write_messages(
        &mut self,
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        global_world_manager: &dyn GlobalWorldManagerType,
        writer: &mut BitWriter,
        packet_index: PacketIndex,
        has_written: &mut bool,
    ) {
        let mut converter = self
            .world_manager
            .entity_converter_mut(global_world_manager);
        self.message_manager.write_messages(
            channel_kinds,
            message_kinds,
            &mut converter,
            writer,
            packet_index,
            has_written,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn write_packet<E: Copy + Eq + Hash + Sync + Send, W: WorldRefType<E>>(
        &mut self,
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        component_kinds: &ComponentKinds,
        now: &Instant,
        writer: &mut BitWriter,
        packet_index: PacketIndex,
        world: &W,
        entity_converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &dyn GlobalWorldManagerType,
        has_written: &mut bool,
        write_world_events: bool,
        host_world_events: &mut VecDeque<(CommandId, EntityCommand)>,
        update_events: &mut HashMap<GlobalEntity, HashSet<ComponentKind>>,
        entity_priority_order: Option<&[GlobalEntity]>,
    ) {
        // write messages
        self.write_messages(
            channel_kinds,
            message_kinds,
            global_world_manager,
            writer,
            packet_index,
            has_written,
        );

        // write world events
        if write_world_events {
            WorldWriter::write_into_packet(
                component_kinds,
                now,
                writer,
                &packet_index,
                world,
                entity_converter,
                global_world_manager,
                &mut self.world_manager,
                has_written,
                host_world_events,
                update_events,
                entity_priority_order,
            );
        }
    }

    pub fn read_packet(
        &mut self,
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        component_kinds: &ComponentKinds,
        tick: &Tick,
        read_world_events: bool,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        // read messages
        self.message_manager.read_messages(
            channel_kinds,
            message_kinds,
            &mut self.world_manager,
            reader,
        )?;

        // read world events
        if read_world_events {
            WorldReader::read_world_events(&mut self.world_manager, component_kinds, tick, reader)?;
        }

        Ok(())
    }
}
