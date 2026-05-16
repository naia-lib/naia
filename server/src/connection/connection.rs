use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::{hash::Hash, net::SocketAddr};

use log::warn;

use naia_shared::{
    BaseConnection, BigMapKey, BitReader, BitWriter, ChannelKinds, ComponentKind, ComponentKinds,
    ConnectionConfig, EntityAndGlobalEntityConverter, EntityCommand, EntityEvent, GlobalEntity,
    GlobalEntityIndex, GlobalEntitySpawner, GlobalWorldManagerType, HostType, Instant, MessageIndex,
    MessageKinds, OutgoingPacket, PacketType, Serde, SerdeErr, SnapshotMap, StandardHeader, Tick,
    WorldMutType, WorldRefType, MTU_SIZE_BYTES,
};

use crate::{
    connection::{
        io::SendIo,
        ping_config::PingConfig,
        recv_connection::RecvConnection,
        send_connection::SendConnection,
        tick_buffer_messages::TickBufferMessages,
    },
    events::WorldEvents,
    request::{GlobalRequestManager, GlobalResponseManager},
    server::connection_shared::ConnectionShared,
    time_manager::TimeManager,
    user::UserKey,
    world::global_world_manager::GlobalWorldManager,
};

cfg_if! {
    if #[cfg(feature = "e2e_debug")] {
        use std::sync::atomic::Ordering;
        use naia_shared::EntityAuthStatus;
        use crate::server::world_server::SERVER_TX_FRAMES;
    }
}

/// Fine-grained timing of `Connection::send_packets` sub-phases. Used by
/// `examples/phase4_tick_internals.rs` to localize per-user cost inside the
/// idle send path. Disabled in release unless `bench_instrumentation`.
#[cfg(feature = "bench_instrumentation")]
pub mod bench_send_counters {
    use std::sync::atomic::{AtomicU64, Ordering};
    #[doc(hidden)] pub static NS_COLLECT_MESSAGES: AtomicU64 = AtomicU64::new(0);
    #[doc(hidden)] pub static NS_TAKE_OUTGOING_EVENTS: AtomicU64 = AtomicU64::new(0);
    #[doc(hidden)] pub static NS_SEND_PACKET_LOOP: AtomicU64 = AtomicU64::new(0);
    /// Time spent in `write_packet` (serialization) inside the send loop.
    #[doc(hidden)] pub static NS_WRITE_PACKET: AtomicU64 = AtomicU64::new(0);
    /// Time spent in `io.send_packet` (transport) inside the send loop.
    #[doc(hidden)] pub static NS_IO_SEND: AtomicU64 = AtomicU64::new(0);
    /// Total data packets written across all connections per tick.
    #[doc(hidden)] pub static N_PACKETS_SENT: AtomicU64 = AtomicU64::new(0);
    /// Time spent in `WorldWriter::write_into_packet` (entity/component serialization) per tick.
    #[doc(hidden)] pub static NS_WRITE_UPDATES: AtomicU64 = AtomicU64::new(0);

    /// Resets all counters to zero.
    pub fn reset() {
        NS_COLLECT_MESSAGES.store(0, Ordering::Relaxed);
        NS_TAKE_OUTGOING_EVENTS.store(0, Ordering::Relaxed);
        NS_SEND_PACKET_LOOP.store(0, Ordering::Relaxed);
        NS_WRITE_PACKET.store(0, Ordering::Relaxed);
        NS_IO_SEND.store(0, Ordering::Relaxed);
        N_PACKETS_SENT.store(0, Ordering::Relaxed);
        NS_WRITE_UPDATES.store(0, Ordering::Relaxed);
        naia_shared::bench_take_events_counters::reset();
        crate::server::world_server::bench_iris_counters::reset();
        naia_shared::bench_write_counters::reset();
    }
    /// Returns a snapshot of all counters as a tuple.
    pub fn snapshot() -> (u64, u64, u64) {
        (
            NS_COLLECT_MESSAGES.load(Ordering::Relaxed),
            NS_TAKE_OUTGOING_EVENTS.load(Ordering::Relaxed),
            NS_SEND_PACKET_LOOP.load(Ordering::Relaxed),
        )
    }
    /// Returns `(write_packet_ns, io_send_ns, n_packets_sent, write_updates_ns)` — send-loop sub-breakdown.
    pub fn snapshot_send_breakdown() -> (u64, u64, u64, u64) {
        (
            NS_WRITE_PACKET.load(Ordering::Relaxed),
            NS_IO_SEND.load(Ordering::Relaxed),
            N_PACKETS_SENT.load(Ordering::Relaxed),
            NS_WRITE_UPDATES.load(Ordering::Relaxed),
        )
    }
}

/// Transitional wrapper holding both halves of a per-user connection.
///
/// In step 4-E, `recv_user_connections: HashMap<SocketAddr, RecvConnection>`
/// will live on `RecvState<E>`, and `send_user_connections: HashMap<SocketAddr,
/// SendConnection>` on `SendState<E>`. `Connection` then disappears —
/// `WorldServer::user_connections` is replaced by the two separately-owned
/// maps. For step 4-D, the wrapper keeps `user_connections` intact while
/// giving every consumer access to the recv/send halves they actually need.
pub struct Connection {
    pub recv: RecvConnection,
    pub send: SendConnection,
}

impl Connection {
    /// Construct a new connection, wiring both halves to share the same
    /// `Arc<ConnectionShared>` and the crossbeam channel between
    /// `AckManagerRecv` and `AckManagerSend`.
    pub fn new(
        connection_config: &ConnectionConfig,
        ping_config: &PingConfig,
        user_address: &SocketAddr,
        user_key: &UserKey,
        channel_kinds: &ChannelKinds,
        global_world_manager: &GlobalWorldManager,
        max_replicated_entities: usize,
    ) -> Self {
        let (base_recv, base_send) = BaseConnection::new_split(
            connection_config,
            &Some(*user_address),
            HostType::Server,
            user_key.to_u64(),
            channel_kinds,
            global_world_manager,
        );
        let shared = Arc::new(ConnectionShared::new());
        Self {
            recv: RecvConnection::new(
                connection_config,
                ping_config,
                *user_address,
                *user_key,
                channel_kinds,
                base_recv,
                Arc::clone(&shared),
            ),
            send: SendConnection::new(
                *user_address,
                *user_key,
                base_send,
                max_replicated_entities,
                shared,
            ),
        }
    }

    /// Remote socket address — same on both halves.
    pub fn address(&self) -> SocketAddr {
        self.recv.address
    }

    /// User key — same on both halves.
    pub fn user_key(&self) -> UserKey {
        self.recv.user_key
    }

    /// Borrow the shared per-connection state (Arc clone available via
    /// `Arc::clone(&conn.shared())`).
    pub fn shared(&self) -> &Arc<ConnectionShared> {
        &self.recv.shared
    }

    /// True when no packet has been received within the disconnect timeout.
    pub fn should_drop(&self) -> bool {
        self.recv.should_drop()
    }

    /// Set entity `idx` as visible for this connection (scope entry or resume).
    pub fn set_entity_visible(&mut self, idx: GlobalEntityIndex) {
        self.send.set_entity_visible(idx);
    }

    /// Clear entity `idx` as not visible for this connection (scope exit or pause).
    pub fn clear_entity_visible(&mut self, idx: GlobalEntityIndex) {
        self.send.clear_entity_visible(idx);
    }

    /// Record the recv-side bookkeeping for an incoming packet header AND
    /// drain the cross-half channel into the send half (matching the
    /// pre-split synchronous flow). In the fully-threaded model (post-4-F),
    /// recv calls only `recv.process_incoming_header`; the send thread runs
    /// `send.drain_acks` at the top of its own send cycle.
    pub fn process_incoming_header(&mut self, header: &StandardHeader) {
        self.recv.process_incoming_header(header);
        self.send.drain_acks(&mut []);
    }

    /// Build the standard header for an outgoing packet, threading the
    /// recv-side ack info from the recv half into the send half's writer.
    pub fn write_header(
        &mut self,
        packet_type: PacketType,
        writer: &mut BitWriter,
    ) -> StandardHeader {
        let last_rx = self.recv.base.ack_recv.last_received_packet_index();
        let ack_bits = self.recv.base.ack_recv.ack_bitfield();
        self.send
            .base
            .write_header_with(packet_type, last_rx, ack_bits, writer)
    }

    #[cfg(feature = "test_utils")]
    pub fn diff_handler_receiver_count(&self) -> usize {
        self.send.base.world_manager.diff_handler_receiver_count()
    }

    #[cfg(feature = "test_utils")]
    pub fn inject_tick_buffer_message(
        &mut self,
        channel_kind: &naia_shared::ChannelKind,
        host_tick: &naia_shared::Tick,
        message_tick: &naia_shared::Tick,
        message: naia_shared::MessageContainer,
    ) -> bool {
        self.recv
            .tick_buffer
            .inject_message(channel_kind, host_tick, message_tick, message)
    }

    /// Read packet data received from a client, storing necessary data in an internal buffer
    #[allow(clippy::too_many_arguments)]
    pub fn read_packet(
        &mut self,
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        component_kinds: &ComponentKinds,
        client_authoritative_entities: bool,
        server_tick: Tick,
        client_tick: Tick,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        // read tick-buffered messages
        self.recv.tick_buffer.read_messages(
            channel_kinds,
            message_kinds,
            &server_tick,
            &client_tick,
            self.send.base.world_manager.entity_converter(),
            reader,
        )?;

        // read common parts of packet (messages & world events) — runs on
        // the coordinator thread that owns SendState in the new tick
        // sequence; lives on the send half per the MessageManager
        // sub-audit (see naia-shared/base_connection.rs).
        self.send.base.read_packet(
            channel_kinds,
            message_kinds,
            component_kinds,
            &client_tick,
            client_authoritative_entities,
            reader,
        )?;

        Ok(())
    }

    /// Receive & process stored packet data
    #[allow(clippy::too_many_arguments)]
    pub fn process_packets<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        message_kinds: &MessageKinds,
        component_kinds: &ComponentKinds,
        client_authoritative_entities: bool,
        now: &Instant,
        global_entity_map: &mut dyn GlobalEntitySpawner<E>,
        global_world_manager: &mut GlobalWorldManager,
        global_request_manager: &mut GlobalRequestManager,
        global_response_manager: &mut GlobalResponseManager,
        world: &mut W,
        incoming_events: &mut WorldEvents<E>,
    ) -> Vec<EntityEvent> {
        let user_key = self.user_key();
        // Receive Message Events
        let (entity_converter, entity_waitlist) =
            self.send.base.world_manager.get_message_processor_helpers();
        let messages = self.send.base.message_manager.receive_messages(
            message_kinds,
            now,
            entity_converter,
            entity_waitlist,
        );
        for (channel_kind, messages) in messages {
            for message in messages {
                incoming_events.push_message(&user_key, &channel_kind, message);
            }
        }

        // Receive Request and Response Events
        let (requests, responses) = self.send.base.message_manager.receive_requests_and_responses();
        // Requests
        for (channel_kind, requests) in requests {
            for (local_response_id, request) in requests {
                let global_response_id = global_response_manager.create_response_id(
                    &user_key,
                    &channel_kind,
                    &local_response_id,
                );
                incoming_events.push_request(
                    &user_key,
                    &channel_kind,
                    global_response_id,
                    request,
                );
            }
        }
        // Responses
        for (global_request_id, response) in responses {
            global_request_manager.receive_response(&global_request_id, response);
        }

        // Receive World Events
        if client_authoritative_entities {
            self.send.base.world_manager.take_incoming_events(
                global_entity_map,
                global_world_manager,
                component_kinds,
                world,
                now,
            )
        } else {
            Vec::new()
        }
    }

    pub fn tick_buffer_messages(&mut self, tick: &Tick, messages: &mut TickBufferMessages) {
        let user_key = self.user_key();
        let channel_messages = self.recv.tick_buffer.receive_messages(tick);
        for (channel_kind, received_messages) in channel_messages {
            for message in received_messages {
                messages.push_message(&user_key, &channel_kind, message);
            }
        }
    }

    // Outgoing data
    #[allow(clippy::too_many_arguments)]
    pub fn send_packets<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        &mut self,
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        component_kinds: &ComponentKinds,
        now: &Instant,
        io: &mut SendIo,
        world: &W,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &GlobalWorldManager,
        time_manager: &TimeManager,
        update_list: &mut Vec<(GlobalEntity, GlobalEntityIndex, E, HashMap<ComponentKind, u16>)>,
        snapshot_map: &SnapshotMap,
    ) {
        let rtt_millis = self.recv.ping_manager.rtt_average;

        #[cfg(feature = "bench_instrumentation")]
        let t = std::time::Instant::now();
        self.send.base.collect_messages(now, &rtt_millis);
        #[cfg(feature = "bench_instrumentation")]
        bench_send_counters::NS_COLLECT_MESSAGES
            .fetch_add(t.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);

        // Collect outgoing entity commands only. Update events are pre-built by the
        // three-phase Iris loop in WorldServer::send_all_packets and passed in directly.
        #[cfg(feature = "bench_instrumentation")]
        let t = std::time::Instant::now();
        let mut host_world_events = self
            .send
            .base
            .world_manager
            .take_outgoing_commands(now, &rtt_millis);
        #[cfg(feature = "bench_instrumentation")]
        bench_send_counters::NS_TAKE_OUTGOING_EVENTS
            .fetch_add(t.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
        #[cfg(feature = "e2e_debug")]
        {
            use crate::server::world_server::{
                SERVER_OUTGOING_CMDS_DRAINED_TOTAL, SERVER_WORLD_MSGS_DRAINED,
            };
            let total_drained = host_world_events.len();
            if total_drained > 0 {
                SERVER_OUTGOING_CMDS_DRAINED_TOTAL.fetch_add(total_drained, Ordering::Relaxed);
                SERVER_WORLD_MSGS_DRAINED.fetch_add(total_drained, Ordering::Relaxed);
            }
        }

        self.send.base.accumulate_bandwidth(now);

        #[cfg(feature = "bench_instrumentation")]
        let t = std::time::Instant::now();
        let mut any_sent = false;
        loop {
            if self.send_packet(
                channel_kinds,
                message_kinds,
                component_kinds,
                now,
                io,
                world,
                converter,
                global_world_manager,
                time_manager,
                &mut host_world_events,
                update_list,
                snapshot_map,
            ) {
                any_sent = true;
            } else {
                break;
            }
        }
        if any_sent {
            self.send.base.mark_sent();
        }
        #[cfg(feature = "bench_instrumentation")]
        bench_send_counters::NS_SEND_PACKET_LOOP
            .fetch_add(t.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
    }

    #[allow(clippy::too_many_arguments)]
    fn send_packet<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        &mut self,
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        component_kinds: &ComponentKinds,
        now: &Instant,
        io: &mut SendIo,
        world: &W,
        entity_converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &GlobalWorldManager,
        time_manager: &TimeManager,
        host_world_events: &mut VecDeque<(MessageIndex, EntityCommand)>,
        update_list: &mut Vec<(GlobalEntity, GlobalEntityIndex, E, HashMap<ComponentKind, u16>)>,
        snapshot_map: &SnapshotMap,
    ) -> bool {
        let has_messages = self.send.base.message_manager.has_outgoing_messages();
        let has_events = !host_world_events.is_empty() || !update_list.is_empty();

        let needs_ack_only = self.send.base.take_should_send_empty_ack();

        if needs_ack_only && !has_messages && !has_events {
            let mut writer = BitWriter::new();
            writer.reserve_bits(3);

            let _header = self.write_header(PacketType::Data, &mut writer);

            let tick = time_manager.current_tick();
            tick.ser(&mut writer);
            time_manager.current_tick_instant().ser(&mut writer);

            false.ser(&mut writer);
            false.ser(&mut writer);
            false.ser(&mut writer);

            let addr = self.address();
            if io.send_packet(&addr, writer.to_packet()).is_err() {
                warn!("Server Error: Cannot send ACK-only packet to {}", &addr);
            } else {
                #[cfg(feature = "e2e_debug")]
                {
                    SERVER_TX_FRAMES.fetch_add(1, Ordering::Relaxed);
                }
            }

            return false;
        }

        if has_events || has_messages {
            if !self.send.base.can_spend_bandwidth(MTU_SIZE_BYTES as u32) {
                self.send.base.record_bandwidth_deferred();
                return false;
            }

            #[cfg(feature = "bench_instrumentation")]
            let t_write = std::time::Instant::now();
            let writer = self.write_packet(
                channel_kinds,
                message_kinds,
                component_kinds,
                now,
                world,
                entity_converter,
                global_world_manager,
                time_manager,
                host_world_events,
                update_list,
                snapshot_map,
            );
            #[cfg(feature = "bench_instrumentation")]
            bench_send_counters::NS_WRITE_PACKET
                .fetch_add(t_write.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);

            let packet = writer.to_packet();
            let packet_bytes = packet.slice().len() as u32;
            #[cfg(feature = "bench_instrumentation")]
            let t_io = std::time::Instant::now();
            let addr = self.address();
            if io.send_packet(&addr, packet).is_err() {
                warn!("Server Error: Cannot send data packet to {}", &addr);
            } else {
                self.send.base.spend_bandwidth(packet_bytes);
                #[cfg(feature = "bench_instrumentation")]
                bench_send_counters::N_PACKETS_SENT
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                #[cfg(feature = "e2e_debug")]
                {
                    SERVER_TX_FRAMES.fetch_add(1, Ordering::Relaxed);
                    use crate::server::world_server::SERVER_WORLD_PKTS_SENT;
                    SERVER_WORLD_PKTS_SENT.fetch_add(1, Ordering::Relaxed);
                }
            }
            #[cfg(feature = "bench_instrumentation")]
            bench_send_counters::NS_IO_SEND
                .fetch_add(t_io.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);

            return true;
        }

        false
    }

    #[allow(clippy::too_many_arguments)]
    fn write_packet<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        &mut self,
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        component_kinds: &ComponentKinds,
        now: &Instant,
        world: &W,
        entity_converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &GlobalWorldManager,
        time_manager: &TimeManager,
        host_world_events: &mut VecDeque<(MessageIndex, EntityCommand)>,
        update_list: &mut Vec<(GlobalEntity, GlobalEntityIndex, E, HashMap<ComponentKind, u16>)>,
        snapshot_map: &SnapshotMap,
    ) -> BitWriter {
        let next_packet_index = self.send.base.next_packet_index();

        let mut writer = BitWriter::new();
        writer.reserve_bits(3);

        self.write_header(PacketType::Data, &mut writer);

        let tick = time_manager.current_tick();
        tick.ser(&mut writer);
        time_manager.current_tick_instant().ser(&mut writer);

        let mut has_written = false;

        #[cfg(feature = "e2e_debug")]
        let set_auth_granted_before = host_world_events
            .iter()
            .filter(|(_, cmd)| {
                if let EntityCommand::SetAuthority(_, _, status) = cmd {
                    *status == EntityAuthStatus::Granted
                } else {
                    false
                }
            })
            .count();

        let diff_handler_arc = global_world_manager.diff_handler();
        let diff_handler_guard = diff_handler_arc.read().expect("GlobalDiffHandler lock poisoned");
        #[cfg(feature = "bench_instrumentation")]
        let t_base_write = std::time::Instant::now();
        self.send.base.write_packet(
            channel_kinds,
            message_kinds,
            component_kinds,
            now,
            &mut writer,
            next_packet_index,
            world,
            entity_converter,
            global_world_manager,
            &mut has_written,
            true,
            host_world_events,
            update_list,
            Some(&*diff_handler_guard),
            Some(snapshot_map),
        );
        #[cfg(feature = "bench_instrumentation")]
        bench_send_counters::NS_WRITE_UPDATES
            .fetch_add(t_base_write.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);

        #[cfg(feature = "e2e_debug")]
        {
            let set_auth_granted_after = host_world_events
                .iter()
                .filter(|(_, cmd)| {
                    if let EntityCommand::SetAuthority(_, _, status) = cmd {
                        *status == EntityAuthStatus::Granted
                    } else {
                        false
                    }
                })
                .count();
            let written_count = set_auth_granted_before - set_auth_granted_after;
            if written_count > 0 {
                use crate::server::world_server::SERVER_WROTE_SET_AUTH;
                SERVER_WROTE_SET_AUTH.fetch_add(written_count, Ordering::Relaxed);
            }
        }

        writer
    }

    /// IO-free variant of `send_packets`. Builds all outgoing packets for this
    /// connection without sending them. Returns `(packets, any_built)`.
    /// Caller flushes each packet via `io.send_packet(&self.address(), pkt)` after
    /// the parallel build phase completes.
    #[allow(clippy::too_many_arguments)]
    pub fn build_all_packets<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        &mut self,
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        component_kinds: &ComponentKinds,
        now: &Instant,
        world: &W,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &GlobalWorldManager,
        time_manager: &TimeManager,
        update_list: &mut Vec<(GlobalEntity, GlobalEntityIndex, E, HashMap<ComponentKind, u16>)>,
        snapshot_map: &SnapshotMap,
    ) -> (Vec<OutgoingPacket>, bool) {
        let rtt_millis = self.recv.ping_manager.rtt_average;
        self.send.base.collect_messages(now, &rtt_millis);
        let mut host_world_events =
            self.send.base.world_manager.take_outgoing_commands(now, &rtt_millis);
        self.send.base.accumulate_bandwidth(now);

        let mut packets = Vec::new();
        loop {
            let Some(pkt) = self.build_one_packet(
                channel_kinds,
                message_kinds,
                component_kinds,
                now,
                world,
                converter,
                global_world_manager,
                time_manager,
                &mut host_world_events,
                update_list,
                snapshot_map,
            ) else {
                break;
            };
            packets.push(pkt);
        }
        let any_built = !packets.is_empty();
        if any_built {
            self.send.base.mark_sent();
        }
        (packets, any_built)
    }

    /// Build one outgoing packet without IO. See `build_all_packets`.
    #[allow(clippy::too_many_arguments)]
    fn build_one_packet<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        &mut self,
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        component_kinds: &ComponentKinds,
        now: &Instant,
        world: &W,
        entity_converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &GlobalWorldManager,
        time_manager: &TimeManager,
        host_world_events: &mut VecDeque<(MessageIndex, EntityCommand)>,
        update_list: &mut Vec<(GlobalEntity, GlobalEntityIndex, E, HashMap<ComponentKind, u16>)>,
        snapshot_map: &SnapshotMap,
    ) -> Option<OutgoingPacket> {
        let has_messages = self.send.base.message_manager.has_outgoing_messages();
        let has_events = !host_world_events.is_empty() || !update_list.is_empty();
        let needs_ack_only = self.send.base.take_should_send_empty_ack();

        if needs_ack_only && !has_messages && !has_events {
            let mut writer = BitWriter::new();
            writer.reserve_bits(3);
            let _header = self.write_header(PacketType::Data, &mut writer);
            time_manager.current_tick().ser(&mut writer);
            time_manager.current_tick_instant().ser(&mut writer);
            false.ser(&mut writer);
            false.ser(&mut writer);
            false.ser(&mut writer);
            return Some(writer.to_packet());
        }

        if has_events || has_messages {
            if !self.send.base.can_spend_bandwidth(MTU_SIZE_BYTES as u32) {
                self.send.base.record_bandwidth_deferred();
                return None;
            }
            let writer = self.write_packet(
                channel_kinds,
                message_kinds,
                component_kinds,
                now,
                world,
                entity_converter,
                global_world_manager,
                time_manager,
                host_world_events,
                update_list,
                snapshot_map,
            );
            let packet = writer.to_packet();
            self.send.base.spend_bandwidth(packet.slice().len() as u32);
            return Some(packet);
        }

        None
    }

    pub fn process_received_commands(&mut self) {
        self.send.base.world_manager.process_delivered_commands();
    }
}
