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

#[cfg(test)]
mod base_connection_tests {
    //! The facade's own behaviour: which half each accessor reaches into, and
    //! the ack fan-out.
    //!
    //! Almost every method here delegates, so the bugs available are bugs of
    //! *wiring*: an outgoing header built from the send half's own index
    //! instead of what the recv half has seen, a delivery notification that
    //! reaches the message manager but not the world manager, a `mark_sent`
    //! that resets the heartbeat but leaves the empty-ack flag standing. The
    //! sub-managers' own semantics are pinned in their own modules; these
    //! tests pin the seams between them.

    use std::{collections::VecDeque, net::SocketAddr, time::Duration};

    use naia_serde::{BitReader, BitWriter, Serde};

    use crate::{
        messages::fragment::{FragmentId, FragmentIndex, FragmentedMessage},
        world::{
            test_support::TestGwm,
            test_world::{TestSpawner, TestWorld},
        },
        Channel, ChannelDirection, ChannelKind, ChannelKinds, ChannelMode, ChannelSettings,
        ComponentKinds, ConnectionConfig, HostType, MessageContainer, MessageKinds, Named,
        PacketIndex, PacketNotifiable, PacketType, ReliableSettings, StandardHeader,
    };

    use super::{BaseConnection, BaseSendConnection};

    /// Records every packet index it is told was delivered.
    #[derive(Default)]
    struct Spy {
        delivered: Vec<PacketIndex>,
    }

    impl PacketNotifiable for Spy {
        fn notify_packet_delivered(&mut self, packet_index: PacketIndex) {
            self.delivered.push(packet_index);
        }
    }

    fn config(heartbeat: Duration) -> ConnectionConfig {
        ConnectionConfig::new(Duration::from_secs(30), heartbeat, None)
    }

    fn connection(config: &ConnectionConfig) -> (TestGwm, BaseConnection) {
        let component_kinds = ComponentKinds::new();
        let gwm = TestGwm::new(&component_kinds);
        let address: Option<SocketAddr> = Some("127.0.0.1:4000".parse().unwrap());
        let connection = BaseConnection::new(
            config,
            &address,
            HostType::Client,
            1,
            &ChannelKinds::new(),
            &gwm,
        );
        (gwm, connection)
    }

    /// A header from the peer that acknowledges `acked` and nothing else.
    fn peer_header(sender_packet_index: PacketIndex, acked: PacketIndex) -> StandardHeader {
        StandardHeader::new(PacketType::Data, sender_packet_index, acked, 0)
    }

    #[test]
    fn an_acked_data_packet_notifies_both_managers_and_the_callers_extras() {
        // The delivery notification is how a reliable sender learns to stop
        // retransmitting. It has to reach the two managers the connection owns
        // *and* whatever the caller passed in -- dropping either half means
        // something retransmits forever.
        let cfg = config(Duration::from_secs(4));
        let (_gwm, mut connection) = connection(&cfg);

        let mut writer = BitWriter::new();
        let sent = connection.write_header(PacketType::Data, &mut writer);

        let mut spy = Spy::default();
        connection
            .process_incoming_header(&peer_header(0, sent.sender_packet_index), &mut [&mut spy]);

        assert_eq!(
            spy.delivered,
            vec![sent.sender_packet_index],
            "the caller's notifiable must be told about the acked packet"
        );
    }

    #[test]
    fn an_ack_for_a_packet_that_was_never_sent_notifies_nothing() {
        // A peer can claim to have received anything. Only indexes this
        // connection actually put on the wire may fire a notification.
        let cfg = config(Duration::from_secs(4));
        let (_gwm, mut connection) = connection(&cfg);

        let mut spy = Spy::default();
        connection.process_incoming_header(&peer_header(0, 40), &mut [&mut spy]);

        assert!(
            spy.delivered.is_empty(),
            "an unsolicited ack must not be forwarded as a delivery"
        );
    }

    #[test]
    fn the_outgoing_header_acknowledges_what_the_recv_half_has_seen() {
        // The ack fields come from the RECV half. Sourcing them from the send
        // half's own counter would have the connection acknowledge its own
        // packets, and the peer would resend everything forever.
        let cfg = config(Duration::from_secs(4));
        let (_gwm, mut connection) = connection(&cfg);

        connection.process_incoming_header(&peer_header(7, 0), &mut []);
        assert_eq!(connection.last_received_packet_index(), 7);

        let mut writer = BitWriter::new();
        let header = connection.write_header(PacketType::Data, &mut writer);
        assert_eq!(
            header.sender_ack_index, 7,
            "the outgoing header must acknowledge the peer's latest packet"
        );
    }

    #[test]
    fn each_written_header_takes_the_next_packet_index() {
        let cfg = config(Duration::from_secs(4));
        let (_gwm, mut connection) = connection(&cfg);

        let first = connection.next_packet_index();
        let mut writer = BitWriter::new();
        let header = connection.write_header(PacketType::Data, &mut writer);
        assert_eq!(header.sender_packet_index, first);
        assert_eq!(
            connection.next_packet_index(),
            first.wrapping_add(1),
            "writing a header must consume the index it stamped"
        );
    }

    #[test]
    fn a_written_header_is_the_header_that_reads_back() {
        let cfg = config(Duration::from_secs(4));
        let (_gwm, mut connection) = connection(&cfg);

        let mut writer = BitWriter::new();
        let header = connection.write_header(PacketType::Data, &mut writer);
        let bytes = writer.to_bytes();

        let read_back = StandardHeader::de(&mut BitReader::new(&bytes))
            .expect("the header just written should parse");
        assert_eq!(read_back, header);
    }

    #[test]
    fn sending_anything_clears_both_the_heartbeat_and_the_empty_ack() {
        // A heartbeat exists only to keep the NAT mapping alive when nothing
        // else is being sent, and an empty ack only to acknowledge when there
        // is no payload to ride along with. Real traffic satisfies both, so
        // mark_sent has to retire both -- otherwise every data packet is
        // chased by a redundant one.
        let cfg = config(Duration::ZERO);
        let (_gwm, mut connection) = connection(&cfg);

        assert!(
            connection.should_send_heartbeat(),
            "a zero interval means one is due immediately"
        );
        connection.mark_should_send_empty_ack();
        assert!(connection.should_send_empty_ack());

        connection.mark_sent();

        assert!(
            !connection.should_send_empty_ack(),
            "real traffic carries the ack, so the empty one is no longer needed"
        );
    }

    #[test]
    fn taking_the_empty_ack_flag_clears_it() {
        // The send loop takes the flag rather than reading it, so two passes
        // over the same connection cannot both send an empty ack.
        let cfg = config(Duration::from_secs(4));
        let (_gwm, mut connection) = connection(&cfg);

        assert!(!connection.take_should_send_empty_ack());
        connection.mark_should_send_empty_ack();
        assert!(connection.take_should_send_empty_ack());
        assert!(
            !connection.take_should_send_empty_ack(),
            "the flag must not survive being taken"
        );
        assert!(!connection.should_send_empty_ack());
    }

    #[test]
    fn the_bandwidth_accessors_reach_the_send_halfs_accumulator() {
        // These are pure delegation; the accumulator's own arithmetic is
        // pinned in its module. What is pinned here is that the facade and the
        // send half address the SAME accumulator.
        let cfg = config(Duration::from_secs(4));
        let (_gwm, mut connection) = connection(&cfg);

        let now = naia_socket_shared::Instant::now();
        connection.accumulate_bandwidth(&now);
        let before = connection.bandwidth_remaining();
        connection.spend_bandwidth(100);
        assert_eq!(
            connection.bandwidth_remaining(),
            before - 100.0,
            "a spend through the facade must debit the send half's budget"
        );
        assert_eq!(
            connection.send.bandwidth_remaining(),
            connection.bandwidth_remaining(),
            "the facade and the send half must not be reading two different budgets"
        );
    }

    #[test]
    fn reading_a_packet_stops_at_the_messages_when_world_events_are_off() {
        // The client reads world events from data packets; the server does not.
        // With the flag off, trailing bytes are left for the caller and must
        // not be parsed as a world event -- a stray byte would otherwise fail
        // the whole packet.
        let cfg = config(Duration::from_secs(4));
        let (_gwm, mut connection) = connection(&cfg);
        let channel_kinds = ChannelKinds::new();
        let message_kinds = MessageKinds::new();
        let component_kinds = ComponentKinds::new();

        // An empty message section (one `false` continue bit) followed by a
        // sentinel the caller expects to still be there.
        let mut writer = BitWriter::new();
        false.ser(&mut writer);
        0xABCDu16.ser(&mut writer);
        let bytes = writer.to_bytes();

        let mut reader = BitReader::new(&bytes);
        connection
            .read_packet(
                &channel_kinds,
                &message_kinds,
                &component_kinds,
                &0,
                false,
                &mut reader,
            )
            .expect("an empty message section should parse");
        assert_eq!(
            u16::de(&mut reader).expect("the sentinel should still be unread"),
            0xABCD,
            "with world events off, the reader must be left where the messages ended"
        );
    }

    #[test]
    fn a_truncated_world_event_section_is_an_error_not_a_panic() {
        // Packet bytes are attacker-controlled. A world-event section that
        // opens an entity and then stops must surface as an error the
        // connection can drop, not a panic that takes the host down.
        let cfg = config(Duration::from_secs(4));
        let (_gwm, mut connection) = connection(&cfg);

        let mut writer = BitWriter::new();
        false.ser(&mut writer); // no messages
        true.ser(&mut writer); // ...but an entity update section that never arrives
        let bytes = writer.to_bytes();

        assert!(
            connection
                .read_packet(
                    &ChannelKinds::new(),
                    &MessageKinds::new(),
                    &ComponentKinds::new(),
                    &0,
                    true,
                    &mut BitReader::new(&bytes),
                )
                .is_err(),
            "a section that stops mid-entity must be rejected"
        );
    }

    struct Wire;
    impl Named for Wire {
        fn name(&self) -> String {
            "Wire".to_string()
        }
        fn protocol_name() -> &'static str {
            "Wire"
        }
    }
    impl Channel for Wire {}

    fn wire_kinds() -> ChannelKinds {
        let mut kinds = ChannelKinds::new();
        kinds.add_channel::<Wire>(ChannelSettings::new(
            ChannelMode::OrderedReliable(ReliableSettings::default()),
            ChannelDirection::Bidirectional,
        ));
        kinds
    }

    fn message_kinds() -> MessageKinds {
        let mut kinds = MessageKinds::new();
        kinds.add_message::<FragmentedMessage>();
        kinds
    }

    fn payload(tag: u8) -> MessageContainer {
        MessageContainer::new(Box::new(FragmentedMessage::new(
            FragmentId::zero(),
            FragmentIndex::from_u32(0),
            vec![tag].into_boxed_slice(),
        )))
    }

    fn wire_connection(
        config: &ConnectionConfig,
        kinds: &ChannelKinds,
    ) -> (TestGwm, BaseConnection) {
        let component_kinds = ComponentKinds::new();
        let gwm = TestGwm::new(&component_kinds);
        let address: Option<SocketAddr> = Some("127.0.0.1:4000".parse().unwrap());
        let connection = BaseConnection::new(config, &address, HostType::Client, 1, kinds, &gwm);
        (gwm, connection)
    }

    #[test]
    fn a_packet_written_by_one_connection_is_read_by_its_peer() {
        // The whole send path in one line: queue, collect, write, read. With
        // world events off the packet is messages only, and the peer must
        // consume exactly what was written -- no world-event section may be
        // demanded of it, and none may be left behind.
        let cfg = config(Duration::from_secs(4));
        let channel_kinds = wire_kinds();
        let messages = message_kinds();
        let component_kinds = ComponentKinds::new();
        let (sender_gwm, mut sender) = wire_connection(&cfg, &channel_kinds);
        let (_peer_gwm, mut peer) = wire_connection(&cfg, &channel_kinds);

        {
            let BaseSendConnection {
                message_manager,
                world_manager,
                ..
            } = &mut sender.send;
            let mut converter = world_manager.entity_converter_mut(&sender_gwm);
            message_manager.send_message(
                &messages,
                &mut converter,
                &ChannelKind::of::<Wire>(),
                payload(9),
            );
        }
        // A reliable sender only offers collected messages to a packet.
        sender.collect_messages(&naia_socket_shared::Instant::now(), &200.0);

        let world = TestWorld::new();
        let spawner = TestSpawner::default();
        let mut writer = BitWriter::new();
        let mut has_written = false;
        let packet_index = sender.next_packet_index();
        sender.write_packet(
            &channel_kinds,
            &messages,
            &component_kinds,
            &naia_socket_shared::Instant::now(),
            &mut writer,
            packet_index,
            &world,
            &spawner,
            &sender_gwm,
            &mut has_written,
            false,
            &mut VecDeque::new(),
            &mut Vec::new(),
            None,
            None,
        );
        assert!(has_written, "a queued message should have been written");

        let bytes = writer.to_bytes();
        peer.read_packet(
            &channel_kinds,
            &messages,
            &component_kinds,
            &0,
            false,
            &mut BitReader::new(&bytes),
        )
        .expect("a packet the peer just wrote should parse");
    }

    #[test]
    fn a_connection_that_has_lost_nothing_reports_no_loss() {
        // packet_loss_pct is read straight into telemetry; a fresh connection
        // that reported anything but zero would look like a broken link.
        let cfg = config(Duration::from_secs(4));
        let (_gwm, connection) = connection(&cfg);
        assert_eq!(connection.packet_loss_pct(), 0.0);
    }

    fn advance(t: &naia_socket_shared::Instant, ms: u32) -> naia_socket_shared::Instant {
        let mut out = t.clone();
        out.add_millis(ms);
        out
    }

    #[test]
    fn the_bandwidth_budget_accumulates_over_time_and_gates_a_send() {
        // The budget gate is the only thing standing between a busy tick and a
        // connection saturating its link. A gate stuck open (or shut) is
        // invisible until production.
        let mut cfg = config(Duration::from_secs(4));
        cfg.bandwidth.target_bytes_per_sec = 1_000;
        let (_gwm, mut connection) = connection(&cfg);

        let t0 = naia_socket_shared::Instant::now();
        connection.accumulate_bandwidth(&t0);
        assert_eq!(
            connection.bandwidth_remaining(),
            0.0,
            "the first tick establishes the baseline, it does not grant budget"
        );

        let t1 = advance(&t0, 1_000);
        connection.accumulate_bandwidth(&t1);
        assert!(
            connection.bandwidth_remaining() > 0.0,
            "a second of elapsed time at 1000 B/s must grant budget"
        );
        assert!(
            connection.can_spend_bandwidth(500),
            "a packet well inside the budget is permitted"
        );

        connection.spend_bandwidth(2_000);
        assert!(
            connection.bandwidth_remaining() < 0.0,
            "the overshoot allowance lets one packet run the budget negative"
        );
        assert!(
            !connection.can_spend_bandwidth(500),
            "and the next packet in the same tick is refused"
        );
    }

    #[test]
    fn bytes_sent_are_reported_for_the_completed_tick() {
        let mut cfg = config(Duration::from_secs(4));
        cfg.bandwidth.target_bytes_per_sec = 1_000;
        let (_gwm, mut connection) = connection(&cfg);

        let t0 = naia_socket_shared::Instant::now();
        connection.accumulate_bandwidth(&t0);
        let t1 = advance(&t0, 1_000);
        connection.accumulate_bandwidth(&t1);
        connection.spend_bandwidth(250);

        // The figure is per completed tick, so it appears once the next tick
        // rolls over -- reporting the in-progress tick would double-count.
        let t2 = advance(&t1, 1_000);
        connection.accumulate_bandwidth(&t2);
        assert_eq!(connection.bandwidth_bytes_sent_last_tick(), 250);
    }

    #[test]
    fn deferred_packets_are_only_counted_under_bench_instrumentation() {
        // record_bandwidth_deferred is called from the send loop on every
        // refused packet, so it must stay callable; the counter behind it is
        // compiled out unless `bench_instrumentation` is on. (A mutant that
        // hardcodes the getter to 0 is equivalent under default features --
        // triaged here rather than once per sweep.)
        let cfg = config(Duration::from_secs(4));
        let (_gwm, mut connection) = connection(&cfg);

        connection.record_bandwidth_deferred();
        let deferred = connection.bandwidth_packets_deferred_last_tick();
        #[cfg(not(feature = "bench_instrumentation"))]
        assert_eq!(deferred, 0);
        #[cfg(feature = "bench_instrumentation")]
        assert_eq!(deferred, 0, "the count is per completed tick");
    }

    #[test]
    fn a_fresh_connection_owes_no_heartbeat() {
        // The interval is what keeps the NAT mapping alive; a connection that
        // claimed a heartbeat was due the instant it opened would send one per
        // tick forever.
        let cfg = config(Duration::from_secs(30));
        let (_gwm, connection) = connection(&cfg);
        assert!(!connection.should_send_heartbeat());
    }

    #[test]
    fn packets_that_fall_out_of_the_ack_window_are_counted_as_lost() {
        // Loss feeds the resend timing. A monitor that always reported zero
        // would make a lossy link look healthy and stall every reliable
        // channel on it.
        let cfg = config(Duration::from_secs(4));
        let (_gwm, mut connection) = connection(&cfg);

        // Put 34 data packets on the wire, then have the peer acknowledge only
        // the last. The 32-packet ack window cannot reach back to the first
        // ones, so they are declared lost.
        let mut last = 0;
        for _ in 0..34 {
            let mut writer = BitWriter::new();
            last = connection
                .write_header(PacketType::Data, &mut writer)
                .sender_packet_index;
        }
        connection.process_incoming_header(&peer_header(0, last), &mut []);

        assert!(
            connection.packet_loss_pct() > 0.0,
            "packets the peer never acknowledged must register as loss"
        );
    }
}
