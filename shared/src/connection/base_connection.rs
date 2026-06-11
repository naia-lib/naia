use std::{collections::VecDeque, hash::Hash, net::SocketAddr};

use naia_serde::{BitReader, BitWriter, Serde, SerdeErr};
use naia_socket_shared::Instant;

use crate::connection::ack_manager::{AckManager, AckManagerRecv, AckManagerSend};
use crate::connection::bandwidth_accumulator::BandwidthAccumulator;
use crate::world::local::local_world_manager::LocalWorldManager;
use crate::world::update::global_diff_handler::GlobalDiffHandler;
use crate::world::world_reader::WorldReader;
use crate::world::world_writer::{SnapshotMap, UpdateKinds, WorldWriter};
use crate::{
    messages::{channels::channel_kinds::ChannelKinds, message_manager::MessageManager},
    types::{HostType, PacketIndex},
    world::{
        entity::entity_converters::GlobalWorldManagerType, host::host_world_manager::CommandId,
    },
    ComponentKinds, ConnectionConfig, EntityAndGlobalEntityConverter, EntityCommand, GlobalEntity,
    GlobalEntityIndex, MessageKinds, PacketNotifiable, PacketType, StandardHeader, Tick, Timer,
    WorldRefType,
};

/// Recv-side half of `BaseConnection` (step 4-C.2).
///
/// Owns only the recv-side ack pipeline. Other recv-exclusive state on
/// the server's `Connection` (e.g. `ping_manager`, `tick_buffer`,
/// `timeout_timer`, `manual_disconnect`) is migrated to a `RecvConnection`
/// wrapper in step 4-C.3.
pub struct BaseRecvConnection {
    /// Inbound-ack pipeline tracking which sent packets the peer has acknowledged.
    pub ack_recv: AckManagerRecv,
}

/// Send-side half of `BaseConnection` (step 4-C.2).
///
/// Owns the channel-routed message queues, the per-connection local world
/// manager, the outbound ack pipeline, the heartbeat timer, and the
/// bandwidth accumulator.
///
/// Per the MessageManager sub-audit called out in the §7 §8 spec: although
/// `MessageManager` contains both `channel_senders` (send-only) and
/// `channel_receivers` (recv-only), the recv-side decode path
/// (`read_messages` / `receive_messages` / `receive_requests_and_responses`)
/// is invoked from the **coordinator** thread (in `process_all_packets` at
/// step 2 of the 12-step tick sequence), not from the recv thread itself.
/// The recv thread only buffers raw packet bytes — see
/// `WorldServer::receive_all_packets`. So `MessageManager` is classified
/// send-side here: it sits next to the data the coordinator + send thread
/// need (it's also a `PacketNotifiable` notifiable that fires from the send
/// path's `AckManagerSend::drain_samples`).
pub struct BaseSendConnection {
    /// Manages channel-routed message send/receive queues for this connection.
    pub message_manager: MessageManager,
    /// Manages entity-level replication state for this connection.
    pub world_manager: LocalWorldManager,
    /// Outbound ack pipeline. `pub` so server-side `SendConnection` can
    /// drain samples from the cross-half channel.
    pub ack_send: AckManagerSend,
    heartbeat_timer: Timer,
    bandwidth_accumulator: BandwidthAccumulator,
}

/// Transitional facade that owns both halves as public sub-fields. Callers
/// access `base.recv.*` for recv-side state (currently only `ack_recv`)
/// and `base.send.*` for send-side state (`message_manager`,
/// `world_manager`, etc.). Step 4-C.3 dissolves this facade by moving
/// each half onto the new `RecvConnection` / `SendConnection` wrappers.
pub struct BaseConnection {
    /// Recv-side half of the connection (ack pipeline only at this stage).
    pub recv: BaseRecvConnection,
    /// Send-side half of the connection (message manager, world manager,
    /// ack send pipeline, heartbeat timer, bandwidth accumulator).
    pub send: BaseSendConnection,
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
        let (recv, send) = Self::new_split(
            connection_config,
            address,
            host_type,
            user_key,
            channel_kinds,
            global_world_manager,
        );
        Self { recv, send }
    }

    /// Construct the two halves of a `BaseConnection` directly, wired by the
    /// shared ack channel. Used by `naia-server::Connection::new` which holds
    /// the halves on `RecvConnection` / `SendConnection` separately and does
    /// not need the `BaseConnection` wrapper.
    pub fn new_split(
        connection_config: &ConnectionConfig,
        address: &Option<SocketAddr>,
        host_type: HostType,
        user_key: u64,
        channel_kinds: &ChannelKinds,
        global_world_manager: &dyn GlobalWorldManagerType,
    ) -> (BaseRecvConnection, BaseSendConnection) {
        let (ack_recv, ack_send) = AckManager::new_split();
        let recv = BaseRecvConnection { ack_recv };
        let send = BaseSendConnection {
            message_manager: MessageManager::new(host_type, channel_kinds),
            world_manager: LocalWorldManager::new(
                address,
                host_type,
                user_key,
                global_world_manager,
            ),
            ack_send,
            heartbeat_timer: Timer::new(connection_config.heartbeat_interval),
            bandwidth_accumulator: BandwidthAccumulator::new(&connection_config.bandwidth),
        };
        (recv, send)
    }

    /// Process an incoming packet header. Recv half handles received-packet
    /// bookkeeping and pushes acked-index samples into the cross-half channel;
    /// send half drains the channel, removes acknowledged entries from
    /// `sent_packets`, and fires delivery notifications on the message and
    /// world managers (plus any caller-supplied extras).
    pub fn process_incoming_header(
        &mut self,
        header: &StandardHeader,
        extra_notifiables: &mut [&mut dyn PacketNotifiable],
    ) {
        self.recv.ack_recv.process_incoming_header(header);
        // Disjoint field borrows: destructure so the borrow checker can see
        // ack_send, message_manager, and world_manager are independent.
        let BaseSendConnection {
            message_manager,
            world_manager,
            ack_send,
            ..
        } = &mut self.send;
        let mut base_notifiables: [&mut dyn PacketNotifiable; 2] = [message_manager, world_manager];
        ack_send.drain_samples(&mut base_notifiables, extra_notifiables);
    }

    /// Returns the sequence index of the last received packet from the remote.
    pub fn last_received_packet_index(&self) -> PacketIndex {
        self.recv.ack_recv.last_received_packet_index()
    }
}

impl BaseSendConnection {
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
        self.ack_send.clear_should_send_empty_ack();
    }

    /// Returns whether a heartbeat message should be sent
    pub fn should_send_heartbeat(&self) -> bool {
        self.heartbeat_timer.ringing()
    }

    // Acks & Headers

    /// Sets the flag requesting that an empty ack packet be sent.
    pub fn mark_should_send_empty_ack(&mut self) {
        self.ack_send.mark_should_send_empty_ack();
    }

    /// Returns `true` if an empty ack should be sent this tick.
    pub fn should_send_empty_ack(&self) -> bool {
        self.ack_send.should_send_empty_ack()
    }

    /// Returns the empty-ack flag and clears it atomically.
    pub fn take_should_send_empty_ack(&mut self) -> bool {
        self.ack_send.take_should_send_empty_ack()
    }

    /// Given a packet payload, start tracking the packet via its index, attach
    /// the appropriate header, and return the packet's resulting underlying
    /// bytes. Requires the recv half's last-received state for the inbound
    /// ack-bitfield fields; callers go through `BaseConnection::write_header`.
    ///
    /// This is the post-4-C.2 form: pure send-side, taking recv-derived
    /// ack info as parameters. Step 4-C.3 will surface those values via
    /// `ConnectionShared` atomics, removing the recv-side reference.
    pub fn write_header_with(
        &mut self,
        packet_type: PacketType,
        last_recv_packet_index: PacketIndex,
        ack_bitfield: u32,
        writer: &mut BitWriter,
    ) -> StandardHeader {
        let header = self.ack_send.next_outgoing_packet_header(
            packet_type,
            last_recv_packet_index,
            ack_bitfield,
        );
        header.ser(writer);
        header
    }

    /// Get the next outgoing packet's index
    pub fn next_packet_index(&self) -> PacketIndex {
        self.ack_send.next_sender_packet_index()
    }

    /// Fraction of sent data-packets that were lost in the last 64-packet window.
    pub fn packet_loss_pct(&self) -> f32 {
        self.ack_send.packet_loss_pct()
    }

    /// Drains pending world-manager and message-manager outbound queues into writeable packets.
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

    /// Serializes messages and world events into `writer` for the outgoing packet at `packet_index`.
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
        update_list: &mut Vec<(GlobalEntity, GlobalEntityIndex, E, UpdateKinds)>,
        global_diff_handler: Option<&GlobalDiffHandler>,
        snapshot_map: Option<&SnapshotMap>,
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
                global_diff_handler,
                &mut self.world_manager,
                has_written,
                host_world_events,
                update_list,
                snapshot_map,
            );
        }
    }

    /// Deserializes an incoming packet, routing messages and world events to their managers.
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

impl BaseConnection {
    /// Convenience wrapper for callers that previously hit `base.write_header`.
    /// Reads the inbound ack-bitfield state from the recv half and forwards
    /// to `BaseSendConnection::write_header_with`.
    pub fn write_header(
        &mut self,
        packet_type: PacketType,
        writer: &mut BitWriter,
    ) -> StandardHeader {
        let last_rx = self.recv.ack_recv.last_received_packet_index();
        let ack_bits = self.recv.ack_recv.ack_bitfield();
        self.send
            .write_header_with(packet_type, last_rx, ack_bits, writer)
    }

    // ----- Transitional delegating accessors for send-side methods -----
    //
    // These keep the pre-split `connection.base.X(..)` call pattern working
    // for the ~50 existing call sites in server + client. Step 4-C.3 lifts
    // these calls up to the new `RecvConnection` / `SendConnection`
    // wrappers; at that point this transitional facade goes away.

    /// Bandwidth: see [`BaseSendConnection::accumulate_bandwidth`].
    pub fn accumulate_bandwidth(&mut self, now: &Instant) {
        self.send.accumulate_bandwidth(now);
    }
    /// Bandwidth: see [`BaseSendConnection::can_spend_bandwidth`].
    pub fn can_spend_bandwidth(&self, b: u32) -> bool {
        self.send.can_spend_bandwidth(b)
    }
    /// Bandwidth: see [`BaseSendConnection::spend_bandwidth`].
    pub fn spend_bandwidth(&mut self, b: u32) {
        self.send.spend_bandwidth(b);
    }
    /// Bandwidth: see [`BaseSendConnection::bandwidth_remaining`].
    pub fn bandwidth_remaining(&self) -> f64 {
        self.send.bandwidth_remaining()
    }
    /// Bandwidth: see [`BaseSendConnection::bandwidth_bytes_sent_last_tick`].
    pub fn bandwidth_bytes_sent_last_tick(&self) -> u64 {
        self.send.bandwidth_bytes_sent_last_tick()
    }
    /// Bandwidth: see [`BaseSendConnection::bandwidth_packets_deferred_last_tick`].
    pub fn bandwidth_packets_deferred_last_tick(&self) -> u32 {
        self.send.bandwidth_packets_deferred_last_tick()
    }
    /// Bandwidth: see [`BaseSendConnection::record_bandwidth_deferred`].
    pub fn record_bandwidth_deferred(&mut self) {
        self.send.record_bandwidth_deferred();
    }
    /// Heartbeat: see [`BaseSendConnection::mark_sent`].
    pub fn mark_sent(&mut self) {
        self.send.mark_sent();
    }
    /// Heartbeat: see [`BaseSendConnection::should_send_heartbeat`].
    pub fn should_send_heartbeat(&self) -> bool {
        self.send.should_send_heartbeat()
    }
    /// Acks: see [`BaseSendConnection::mark_should_send_empty_ack`].
    pub fn mark_should_send_empty_ack(&mut self) {
        self.send.mark_should_send_empty_ack();
    }
    /// Acks: see [`BaseSendConnection::should_send_empty_ack`].
    pub fn should_send_empty_ack(&self) -> bool {
        self.send.should_send_empty_ack()
    }
    /// Acks: see [`BaseSendConnection::take_should_send_empty_ack`].
    pub fn take_should_send_empty_ack(&mut self) -> bool {
        self.send.take_should_send_empty_ack()
    }
    /// Packet info: see [`BaseSendConnection::next_packet_index`].
    pub fn next_packet_index(&self) -> PacketIndex {
        self.send.next_packet_index()
    }
    /// Packet info: see [`BaseSendConnection::packet_loss_pct`].
    pub fn packet_loss_pct(&self) -> f32 {
        self.send.packet_loss_pct()
    }
    /// Send pipeline: see [`BaseSendConnection::collect_messages`].
    pub fn collect_messages(&mut self, now: &Instant, rtt_millis: &f32) {
        self.send.collect_messages(now, rtt_millis);
    }
    /// Read pipeline: see [`BaseSendConnection::read_packet`].
    pub fn read_packet(
        &mut self,
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        component_kinds: &ComponentKinds,
        tick: &Tick,
        read_world_events: bool,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        self.send.read_packet(
            channel_kinds,
            message_kinds,
            component_kinds,
            tick,
            read_world_events,
            reader,
        )
    }
    /// Write pipeline: see [`BaseSendConnection::write_packet`].
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
        update_list: &mut Vec<(GlobalEntity, GlobalEntityIndex, E, UpdateKinds)>,
        global_diff_handler: Option<&GlobalDiffHandler>,
        snapshot_map: Option<&SnapshotMap>,
    ) {
        self.send.write_packet(
            channel_kinds,
            message_kinds,
            component_kinds,
            now,
            writer,
            packet_index,
            world,
            entity_converter,
            global_world_manager,
            has_written,
            write_world_events,
            host_world_events,
            update_list,
            global_diff_handler,
            snapshot_map,
        );
    }
}
