use std::collections::{HashMap, HashSet, VecDeque};
use std::{hash::Hash, net::SocketAddr};

use log::warn;

use naia_shared::{
    BaseConnection, BigMapKey, BitReader, BitWriter, ChannelKinds, ComponentKind, ComponentKinds,
    ConnectionConfig, ConnectionVisibilityBitset, EntityAndGlobalEntityConverter, EntityCommand,
    EntityEvent, GlobalEntity, GlobalEntityIndex, GlobalEntitySpawner, GlobalWorldManagerType,
    HostType, Instant, MessageIndex, MessageKinds, OutgoingPriorityHook, PacketType, Serde,
    SerdeErr, SnapshotMap, StandardHeader, Tick, Timer, WorldMutType, WorldRefType, MTU_SIZE_BYTES,
};

use crate::{
    connection::{
        io::Io, ping_config::PingConfig, ping_manager::PingManager,
        tick_buffer_messages::TickBufferMessages, tick_buffer_receiver::TickBufferReceiver,
    },
    events::WorldEvents,
    request::{GlobalRequestManager, GlobalResponseManager},
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

    /// Resets all counters to zero.
    pub fn reset() {
        NS_COLLECT_MESSAGES.store(0, Ordering::Relaxed);
        NS_TAKE_OUTGOING_EVENTS.store(0, Ordering::Relaxed);
        NS_SEND_PACKET_LOOP.store(0, Ordering::Relaxed);
        // The take_outgoing_events breakdown lives in naia-shared
        // (bench_take_events_counters) because the interesting work is
        // inside LocalWorldManager.
        naia_shared::bench_take_events_counters::reset();
    }
    /// Returns a snapshot of all counters as a tuple.
    pub fn snapshot() -> (u64, u64, u64) {
        (
            NS_COLLECT_MESSAGES.load(Ordering::Relaxed),
            NS_TAKE_OUTGOING_EVENTS.load(Ordering::Relaxed),
            NS_SEND_PACKET_LOOP.load(Ordering::Relaxed),
        )
    }
}

pub struct Connection {
    pub address: SocketAddr,
    pub user_key: UserKey,
    pub base: BaseConnection,
    pub ping_manager: PingManager,
    tick_buffer: TickBufferReceiver,
    pub manual_disconnect: bool,
    timeout_timer: Timer,
    /// Per-connection entity visibility bitset. One bit per `GlobalEntityIndex`.
    /// Set when an entity enters scope; cleared on despawn or pause.
    pub visibility: ConnectionVisibilityBitset,
}

impl Connection {
    pub fn new(
        connection_config: &ConnectionConfig,
        ping_config: &PingConfig,
        user_address: &SocketAddr,
        user_key: &UserKey,
        channel_kinds: &ChannelKinds,
        global_world_manager: &GlobalWorldManager,
        max_replicated_entities: usize,
    ) -> Self {
        Self {
            address: *user_address,
            user_key: *user_key,
            base: BaseConnection::new(
                connection_config,
                &Some(*user_address),
                HostType::Server,
                user_key.to_u64(),
                channel_kinds,
                global_world_manager,
            ),
            ping_manager: PingManager::new(ping_config),
            tick_buffer: TickBufferReceiver::new(channel_kinds),
            manual_disconnect: false,
            timeout_timer: Timer::new(connection_config.disconnection_timeout_duration),
            // capacity = max_replicated_entities + 1 (slot 0 = INVALID sentinel)
            visibility: ConnectionVisibilityBitset::new(max_replicated_entities + 1),
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

    /// Returns true when no packet has been received for longer than the
    /// configured `disconnection_timeout_duration`.
    pub fn should_drop(&self) -> bool {
        self.timeout_timer.ringing()
    }

    // Incoming Data

    pub fn process_incoming_header(&mut self, header: &StandardHeader) {
        // Note: identity print is now in world_server::read_data_packet for consistency
        self.base.process_incoming_header(header, &mut []);
        self.timeout_timer.reset();
    }

    #[cfg(feature = "test_utils")]
    pub fn diff_handler_receiver_count(&self) -> usize {
        self.base.world_manager.diff_handler_receiver_count()
    }

    #[cfg(feature = "test_utils")]
    pub fn inject_tick_buffer_message(
        &mut self,
        channel_kind: &naia_shared::ChannelKind,
        host_tick: &naia_shared::Tick,
        message_tick: &naia_shared::Tick,
        message: naia_shared::MessageContainer,
    ) -> bool {
        self.tick_buffer
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
        self.tick_buffer.read_messages(
            channel_kinds,
            message_kinds,
            &server_tick,
            &client_tick,
            self.base.world_manager.entity_converter(),
            reader,
        )?;

        // read common parts of packet (messages & world events)
        self.base.read_packet(
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
        // Receive Message Events
        let (entity_converter, entity_waitlist) =
            self.base.world_manager.get_message_processor_helpers();
        let messages = self.base.message_manager.receive_messages(
            message_kinds,
            now,
            entity_converter,
            entity_waitlist,
        );
        for (channel_kind, messages) in messages {
            for message in messages {
                incoming_events.push_message(&self.user_key, &channel_kind, message);
            }
        }

        // Receive Request and Response Events
        let (requests, responses) = self.base.message_manager.receive_requests_and_responses();
        // Requests
        for (channel_kind, requests) in requests {
            for (local_response_id, request) in requests {
                let global_response_id = global_response_manager.create_response_id(
                    &self.user_key,
                    &channel_kind,
                    &local_response_id,
                );
                incoming_events.push_request(
                    &self.user_key,
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

    pub fn tick_buffer_messages(&mut self, tick: &Tick, messages: &mut TickBufferMessages) {
        let channel_messages = self.tick_buffer.receive_messages(tick);
        for (channel_kind, received_messages) in channel_messages {
            for message in received_messages {
                messages.push_message(&self.user_key, &channel_kind, message);
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
        io: &mut Io,
        world: &W,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &GlobalWorldManager,
        time_manager: &TimeManager,
        priority_hook: &mut dyn OutgoingPriorityHook,
        snapshot_map: &mut SnapshotMap,
    ) {
        let rtt_millis = self.ping_manager.rtt_average;

        #[cfg(feature = "bench_instrumentation")]
        let t = std::time::Instant::now();
        self.base.collect_messages(now, &rtt_millis);
        #[cfg(feature = "bench_instrumentation")]
        bench_send_counters::NS_COLLECT_MESSAGES
            .fetch_add(t.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);

        #[cfg(feature = "bench_instrumentation")]
        let t = std::time::Instant::now();
        let (mut host_world_events, mut update_events) = self
            .base
            .world_manager
            .take_outgoing_events(now, &rtt_millis, world, converter, global_world_manager);
        #[cfg(feature = "bench_instrumentation")]
        bench_send_counters::NS_TAKE_OUTGOING_EVENTS
            .fetch_add(t.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
        // Count drained commands and messages for this connection
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

        // Phase A: tick the outbound token-bucket bandwidth accumulator
        // before the send cycle. Refreshes budget + one-packet overshoot.
        self.base.accumulate_bandwidth(now);

        // Phase B: advance the per-user priority accumulator for every dirty
        // entity bundle this tick (canonical `accumulator += effective_gain`
        // rule from PRIORITY_ACCUMULATOR_PLAN.md III.7.1), then sort entities
        // descending by accumulated priority. The k-way merge inside
        // `write_updates` consumes this order to emit highest-priority bundles
        // first. Captured `initial_dirty` is diffed against `update_events`
        // after the loop to detect drained bundles for reset.
        let initial_dirty: Vec<GlobalEntity> = update_events.keys().copied().collect();
        let mut prioritized: Vec<(GlobalEntity, f32)> = initial_dirty
            .iter()
            .map(|e| (*e, priority_hook.advance(e)))
            .collect();
        prioritized.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let entity_priority_order: Vec<GlobalEntity> =
            prioritized.into_iter().map(|(e, _)| e).collect();

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
                &mut update_events,
                Some(&entity_priority_order),
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

        // Phase B (reset): any entity that was dirty entering the loop but is
        // no longer in `update_events` had its bundle fully drained onto the
        // wire — apply the canonical reset-on-send rule (III.7.5).
        let current_tick = time_manager.current_tick();
        for entity in &initial_dirty {
            if !update_events.contains_key(entity) {
                priority_hook.reset_after_send(entity, current_tick as u32);
            }
        }
        #[cfg(feature = "bench_instrumentation")]
        bench_send_counters::NS_SEND_PACKET_LOOP
            .fetch_add(t.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
    }

    /// Send any message, component actions and component updates to the client
    /// Will split the data into multiple packets.
    #[allow(clippy::too_many_arguments)]
    fn send_packet<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        &mut self,
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        component_kinds: &ComponentKinds,
        now: &Instant,
        io: &mut Io,
        world: &W,
        entity_converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &GlobalWorldManager,
        time_manager: &TimeManager,
        host_world_events: &mut VecDeque<(MessageIndex, EntityCommand)>,
        update_events: &mut HashMap<GlobalEntity, HashSet<ComponentKind>>,
        entity_priority_order: Option<&[GlobalEntity]>,
        snapshot_map: &mut SnapshotMap,
    ) -> bool {
        let has_messages = self.base.message_manager.has_outgoing_messages();
        let has_events = !host_world_events.is_empty() || !update_events.is_empty();

        // Check one-shot ACK flag (edge-triggered, consumed here)
        let needs_ack_only = self.base.take_should_send_empty_ack();

        // If ACK-only and no messages/events, send exactly ONE header-only packet and return false
        if needs_ack_only && !has_messages && !has_events {
            let mut writer = BitWriter::new();
            writer.reserve_bits(3); // Messages, Updates, Actions finish bits

            // write header
            let _header = self.base.write_header(PacketType::Data, &mut writer);

            // write server tick
            let tick = time_manager.current_tick();
            tick.ser(&mut writer);

            // write server tick instant
            time_manager.current_tick_instant().ser(&mut writer);

            // write finish bits for empty packet (no messages, no updates, no actions)
            false.ser(&mut writer); // Messages finish bit
            false.ser(&mut writer); // Updates finish bit
            false.ser(&mut writer); // Actions finish bit

            // send packet
            if io.send_packet(&self.address, writer.to_packet()).is_err() {
                warn!(
                    "Server Error: Cannot send ACK-only packet to {}",
                    &self.address
                );
            } else {
                #[cfg(feature = "e2e_debug")]
                {
                    SERVER_TX_FRAMES.fetch_add(1, Ordering::Relaxed);
                }
            }

            // Return false to stop the loop (ACK-only is one-shot)
            return false;
        }

        // Normal packet sending path (with messages/events or no ACK needed)
        if has_events || has_messages {
            // Phase A: bandwidth budget gate — if we can't afford another MTU
            // packet under the token-bucket (one-packet overshoot included),
            // defer the remaining work to the next tick. Anything unsent
            // compounds; starvation is structurally impossible per Fiedler.
            if !self.base.can_spend_bandwidth(MTU_SIZE_BYTES as u32) {
                self.base.record_bandwidth_deferred();
                return false;
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
                update_events,
                entity_priority_order,
                snapshot_map,
            );

            // send packet, measuring actual size before the move so we can
            // spend exactly what went on the wire.
            let packet = writer.to_packet();
            let packet_bytes = packet.slice().len() as u32;
            if io.send_packet(&self.address, packet).is_err() {
                warn!("Server Error: Cannot send data packet to {}", &self.address);
            } else {
                self.base.spend_bandwidth(packet_bytes);
                #[cfg(feature = "e2e_debug")]
                {
                    SERVER_TX_FRAMES.fetch_add(1, Ordering::Relaxed);
                    use crate::server::world_server::SERVER_WORLD_PKTS_SENT;
                    SERVER_WORLD_PKTS_SENT.fetch_add(1, Ordering::Relaxed);
                }
            }

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
        update_events: &mut HashMap<GlobalEntity, HashSet<ComponentKind>>,
        entity_priority_order: Option<&[GlobalEntity]>,
        snapshot_map: &mut SnapshotMap,
    ) -> BitWriter {
        let next_packet_index = self.base.next_packet_index();

        let mut writer = BitWriter::new();

        // Reserve bits we know will be required to finish the message:
        // 1. Messages finish bit
        // 2. Updates finish bit
        // 3. Actions finish bit
        writer.reserve_bits(3);

        // write header
        self.base.write_header(PacketType::Data, &mut writer);

        // write server tick
        let tick = time_manager.current_tick();
        tick.ser(&mut writer);

        // write server tick instant
        time_manager.current_tick_instant().ser(&mut writer);

        // write common data packet
        let mut has_written = false;

        // Count SetAuthority(Granted) commands before writing
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
            update_events,
            entity_priority_order,
            Some(&*diff_handler_guard),
            Some(snapshot_map),
        );

        #[cfg(feature = "e2e_debug")]
        {
            // Count SetAuthority(Granted) commands after writing (they're consumed during write)
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

            // The difference is how many were written
            let written_count = set_auth_granted_before - set_auth_granted_after;
            if written_count > 0 {
                use crate::server::world_server::SERVER_WROTE_SET_AUTH;
                SERVER_WROTE_SET_AUTH.fetch_add(written_count, Ordering::Relaxed);
            }
        }

        writer
    }

    pub fn process_received_commands(&mut self) {
        self.base.world_manager.process_delivered_commands();
    }
}
