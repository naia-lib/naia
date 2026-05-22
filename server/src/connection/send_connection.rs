//! Send-side half of a per-user `Connection` (step 4-D / 4-E.2d).
//!
//! Owns send-thread-exclusive state: `BaseSendConnection` (message
//! manager, world manager, ack send, heartbeat timer, bandwidth
//! accumulator), `visibility` bitset, and a shared
//! `Arc<ConnectionShared>` cell crossing the recv/send boundary.
//!
//! After 4-E.2d, the methods previously defined on the `Connection`
//! wrapper that touched only the send half (or that read ACK info from
//! the cross-half `ConnectionShared` atomic) live here. See
//! `RecvConnection` for the recv-only methods. Composite call sites
//! that need both halves split-borrow the recv/send maps.

use std::{
    collections::VecDeque,
    hash::Hash,
    net::SocketAddr,
    sync::Arc,
};

use log::warn;

use naia_shared::{
    BaseSendConnection, BitReader, BitWriter, ChannelKinds, ComponentKinds,
    ConnectionVisibilityBitset, EntityAndGlobalEntityConverter, EntityCommand, EntityEvent,
    GlobalEntity, GlobalEntityIndex, GlobalEntitySpawner, GlobalWorldManagerType, MessageIndex,
    MessageKinds, OutgoingPacket, PacketNotifiable, PacketType, Serde, SerdeErr, SnapshotMap,
    StandardHeader, Tick, UpdateKinds, WorldMutType, WorldRefType, MTU_SIZE_BYTES,
};

#[cfg(feature = "bench_instrumentation")]
use crate::connection::connection::bench_send_counters;
use crate::{
    connection::io::SendIo,
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

/// Send-side half of a server-side `Connection`.
pub struct SendConnection {
    /// Remote address of the user.
    pub address: SocketAddr,
    /// User key for this connection.
    pub user_key: UserKey,
    /// Send-side half of the base connection.
    pub base: BaseSendConnection,
    /// Per-connection entity visibility bitset. One bit per `GlobalEntityIndex`.
    /// Set when an entity enters scope; cleared on despawn or pause.
    pub visibility: ConnectionVisibilityBitset,
    /// Shared per-connection state crossing the recv/send boundary.
    pub shared: Arc<ConnectionShared>,
}

impl SendConnection {
    /// Construct a new send half. Takes the send-side `BaseSendConnection`
    /// pre-built by [`naia_shared::BaseConnection::new_split`] (so the
    /// crossbeam channel is shared with the matching `RecvConnection`).
    pub fn new(
        user_address: SocketAddr,
        user_key: UserKey,
        base: BaseSendConnection,
        max_replicated_entities: usize,
        shared: Arc<ConnectionShared>,
    ) -> Self {
        Self {
            address: user_address,
            user_key,
            base,
            // capacity = max_replicated_entities + 1 (slot 0 = INVALID sentinel)
            visibility: ConnectionVisibilityBitset::new(max_replicated_entities + 1),
            shared,
        }
    }

    /// Set entity `idx` as visible for this connection (scope entry or resume).
    pub fn set_entity_visible(&mut self, idx: GlobalEntityIndex) {
        self.visibility.set(idx);
    }

    /// Clear entity `idx` as not visible for this connection (scope exit or pause).
    pub fn clear_entity_visible(&mut self, idx: GlobalEntityIndex) {
        self.visibility.clear(idx);
    }

    /// Drain pending acked-index samples from the cross-half channel,
    /// removing acknowledged entries from `sent_packets`, updating the
    /// loss monitor, and firing delivery notifications on the message
    /// manager and world manager.
    pub fn drain_acks(&mut self, extras: &mut [&mut dyn PacketNotifiable]) {
        let naia_shared::BaseSendConnection {
            message_manager,
            world_manager,
            ack_send,
            ..
        } = &mut self.base;
        let mut base_notifiables: [&mut dyn PacketNotifiable; 2] =
            [message_manager, world_manager];
        ack_send.drain_samples(&mut base_notifiables, extras);
    }

    /// Build the standard header for an outgoing packet. Reads the
    /// recv-derived ACK info from the shared atomic published by
    /// `RecvConnection::process_incoming_header`.
    pub fn write_header(
        &mut self,
        packet_type: PacketType,
        writer: &mut BitWriter,
    ) -> StandardHeader {
        let last_rx = self.shared.remote_ack_seq();
        let ack_bits = self.shared.remote_ack_bitfield();
        self.base.write_header_with(packet_type, last_rx, ack_bits, writer)
    }

    /// Finalize delivered commands after receive — runs once per recv cycle
    /// for every address that delivered a packet.
    pub fn process_received_commands(&mut self) {
        self.base.world_manager.process_delivered_commands();
    }

    /// Receive & process stored packet data. Decodes message / request /
    /// response / world events from the inbound packet buffer. The recv-side
    /// tick-buffer read happens at the call site (recv map lookup) before
    /// this method runs.
    #[allow(clippy::too_many_arguments)]
    pub fn process_packets<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        message_kinds: &MessageKinds,
        component_kinds: &ComponentKinds,
        client_authoritative_entities: bool,
        now: &naia_shared::Instant,
        global_entity_map: &mut dyn GlobalEntitySpawner<E>,
        global_world_manager: &mut GlobalWorldManager,
        global_request_manager: &mut GlobalRequestManager,
        global_response_manager: &mut GlobalResponseManager,
        world: &mut W,
        incoming_events: &mut WorldEvents<E>,
    ) -> Vec<EntityEvent> {
        let user_key = self.user_key;
        // Receive Message Events. Scope the entity-map read guard (held by the
        // converter) so it drops before the &mut world_manager access below.
        let messages = {
            let (entity_converter, entity_waitlist) =
                self.base.world_manager.get_message_processor_helpers();
            self.base.message_manager.receive_messages(
                message_kinds,
                now,
                &entity_converter,
                entity_waitlist,
            )
        };
        for (channel_kind, messages) in messages {
            for message in messages {
                incoming_events.push_message(&user_key, &channel_kind, message);
            }
        }

        // Receive Request and Response Events
        let (requests, responses) = self.base.message_manager.receive_requests_and_responses();
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
        for (global_request_id, response) in responses {
            global_request_manager.receive_response(&global_request_id, response);
        }

        // Receive World Events
        if client_authoritative_entities {
            self.base.world_manager.take_incoming_events(
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

    /// Outgoing data path. `rtt_millis` is supplied by the caller (reads
    /// from the matching `RecvConnection.ping_manager.rtt_average` while
    /// the connection is still on the same thread; in pipeline mode the
    /// recv path publishes RTT into `ConnectionShared` for the send
    /// thread to read).
    #[allow(clippy::too_many_arguments)]
    pub fn send_packets<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        &mut self,
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        component_kinds: &ComponentKinds,
        now: &naia_shared::Instant,
        io: &mut SendIo,
        world: &W,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &GlobalWorldManager,
        time_manager: &TimeManager,
        rtt_millis: f32,
        update_list: &mut Vec<(GlobalEntity, GlobalEntityIndex, E, UpdateKinds)>,
        snapshot_map: &SnapshotMap,
    ) {
        #[cfg(feature = "bench_instrumentation")]
        let t = std::time::Instant::now();
        self.base.collect_messages(now, &rtt_millis);
        #[cfg(feature = "bench_instrumentation")]
        bench_send_counters::NS_COLLECT_MESSAGES
            .fetch_add(t.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);

        // Collect outgoing entity commands only. Update events are pre-built by the
        // three-phase Iris loop in WorldServer::send_all_packets and passed in directly.
        #[cfg(feature = "bench_instrumentation")]
        let t = std::time::Instant::now();
        let mut host_world_events = self
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

        self.base.accumulate_bandwidth(now);

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
            self.base.mark_sent();
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
        now: &naia_shared::Instant,
        io: &mut SendIo,
        world: &W,
        entity_converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &GlobalWorldManager,
        time_manager: &TimeManager,
        host_world_events: &mut VecDeque<(MessageIndex, EntityCommand)>,
        update_list: &mut Vec<(GlobalEntity, GlobalEntityIndex, E, UpdateKinds)>,
        snapshot_map: &SnapshotMap,
    ) -> bool {
        let has_messages = self.base.message_manager.has_outgoing_messages();
        let has_events = !host_world_events.is_empty() || !update_list.is_empty();

        let needs_ack_only = self.base.take_should_send_empty_ack();

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

            let addr = self.address;
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
            if !self.base.can_spend_bandwidth(MTU_SIZE_BYTES as u32) {
                self.base.record_bandwidth_deferred();
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
            let addr = self.address;
            if io.send_packet(&addr, packet).is_err() {
                warn!("Server Error: Cannot send data packet to {}", &addr);
            } else {
                self.base.spend_bandwidth(packet_bytes);
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
        now: &naia_shared::Instant,
        world: &W,
        entity_converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &GlobalWorldManager,
        time_manager: &TimeManager,
        host_world_events: &mut VecDeque<(MessageIndex, EntityCommand)>,
        update_list: &mut Vec<(GlobalEntity, GlobalEntityIndex, E, UpdateKinds)>,
        snapshot_map: &SnapshotMap,
    ) -> BitWriter {
        let next_packet_index = self.base.next_packet_index();

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
        self.base.write_packet(
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
    /// `rtt_millis` is supplied by the caller (see `send_packets`).
    #[allow(clippy::too_many_arguments)]
    pub fn build_all_packets<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        &mut self,
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        component_kinds: &ComponentKinds,
        now: &naia_shared::Instant,
        world: &W,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &GlobalWorldManager,
        time_manager: &TimeManager,
        rtt_millis: f32,
        update_list: &mut Vec<(GlobalEntity, GlobalEntityIndex, E, UpdateKinds)>,
        snapshot_map: &SnapshotMap,
    ) -> (Vec<OutgoingPacket>, bool) {
        self.base.collect_messages(now, &rtt_millis);
        let mut host_world_events =
            self.base.world_manager.take_outgoing_commands(now, &rtt_millis);
        self.base.accumulate_bandwidth(now);

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
            self.base.mark_sent();
        }
        (packets, any_built)
    }

    #[allow(clippy::too_many_arguments)]
    fn build_one_packet<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        &mut self,
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        component_kinds: &ComponentKinds,
        now: &naia_shared::Instant,
        world: &W,
        entity_converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &GlobalWorldManager,
        time_manager: &TimeManager,
        host_world_events: &mut VecDeque<(MessageIndex, EntityCommand)>,
        update_list: &mut Vec<(GlobalEntity, GlobalEntityIndex, E, UpdateKinds)>,
        snapshot_map: &SnapshotMap,
    ) -> Option<OutgoingPacket> {
        let has_messages = self.base.message_manager.has_outgoing_messages();
        let has_events = !host_world_events.is_empty() || !update_list.is_empty();
        let needs_ack_only = self.base.take_should_send_empty_ack();

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
            if !self.base.can_spend_bandwidth(MTU_SIZE_BYTES as u32) {
                self.base.record_bandwidth_deferred();
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
            self.base.spend_bandwidth(packet.slice().len() as u32);
            return Some(packet);
        }

        None
    }

    /// Decode tick-buffer-side messages from an inbound data packet body.
    /// Recv-side path; runs after the standard header has already been
    /// processed via `RecvConnection::process_incoming_header`.
    ///
    /// This is the send-side half of the historical
    /// `Connection::read_packet`: it decodes the message/world section
    /// using the send-side message manager. Tick-buffer reads are
    /// handled at the call site (in the matching `RecvConnection`).
    #[allow(clippy::too_many_arguments)]
    pub fn read_data_section(
        &mut self,
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        component_kinds: &ComponentKinds,
        client_authoritative_entities: bool,
        client_tick: Tick,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        self.base.read_packet(
            channel_kinds,
            message_kinds,
            component_kinds,
            &client_tick,
            client_authoritative_entities,
            reader,
        )
    }
}

#[cfg(feature = "test_utils")]
impl SendConnection {
    #[doc(hidden)]
    pub fn diff_handler_receiver_count(&self) -> usize {
        self.base.world_manager.diff_handler_receiver_count()
    }
}
