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

use std::{collections::VecDeque, hash::Hash, net::SocketAddr, sync::Arc};

use naia_shared::{
    BaseSendConnection, BitReader, BitWriter, CauseKind, ChannelKinds, CommandId, ComponentKind,
    ComponentKinds, ConnectionVisibilityBitset, EntityAndGlobalEntityConverter, EntityCommand,
    EntityEvent, EvidenceEntry, GlobalEntity, GlobalEntityIndex, GlobalEntitySpawner,
    GlobalWorldManagerType, MessageIndex, MessageKinds, OutgoingPacket, PacketNotifiable,
    PacketType, Serde, SerdeErr, SnapshotMap, StandardHeader, Tick, UpdateKinds, WorldMutType,
    WorldRefType, MTU_SIZE_BYTES, REDUNDANT_PACKET_ACKS_SIZE,
};

/// Cap on packets built in one send pass, matched to what a single remote header
/// can acknowledge (1 explicit + a 32-bit bitfield). See `build_all_packets`.
const MAX_PACKETS_PER_SEND_PASS: usize = REDUNDANT_PACKET_ACKS_SIZE as usize + 1;

/// Which of the three walls ended each send pass.
///
/// `build_all_packets` can terminate in exactly three ways, and they are mutually
/// exclusive within one pass: the loop hits [`MAX_PACKETS_PER_SEND_PASS`]
/// (ack-window-limited), `build_one_packet` refuses on bandwidth
/// (bandwidth-limited), or it refuses because nothing is queued (supply-limited).
/// Classifying each pass is the only way to tell "the ack window is the wall" from
/// "there was never enough to send" — a MEAN packets-per-pass cannot, because tile
/// streaming is bursty and idle passes dominate the denominator.
#[cfg(feature = "e2e_debug")]
pub mod send_pass_walls {
    use std::sync::atomic::AtomicUsize;

    /// Passes that built the full `MAX_PACKETS_PER_SEND_PASS` — the ack window bound them.
    pub static ACK_LIMITED: AtomicUsize = AtomicUsize::new(0);
    /// Passes ended early by the bandwidth budget refusing a full MTU.
    pub static BANDWIDTH_LIMITED: AtomicUsize = AtomicUsize::new(0);
    /// Passes ended early because nothing was queued to send.
    pub static SUPPLY_LIMITED: AtomicUsize = AtomicUsize::new(0);
    /// Largest packet count any single pass reached. A mean hides burst saturation;
    /// this does not.
    pub static MAX_PACKETS_IN_A_PASS: AtomicUsize = AtomicUsize::new(0);
}

#[cfg(feature = "bench_instrumentation")]
use crate::connection::connection::bench_send_counters;
use crate::{
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
    /// Latest tick serialized into Data. Quiet links still need one Data
    /// boundary per tick so the client's delivered-state ceiling can advance.
    last_data_tick: Option<Tick>,
    /// Last CommandId journaled as command evidence for this observer (Ada
    /// eighth read: first-write-only journaling). Reliable ids move forward
    /// per observer, so anything at or behind the mark is a resend, not a
    /// new cause. One connection carries one reliable command index space.
    last_journaled_command_id: Option<CommandId>,
}

/// One applied-cause evidence entry for a written entity command: the matcher
/// kind, the commanded entity, the component kind when the cause is
/// component-scoped, and the packet tick that carried it.
pub(crate) type CommandEvidence = EvidenceEntry<GlobalEntity>;

/// Fold written wire commands into command evidence (nydus-dm9 Fix2 + Ada
/// eighth read). Each written command journals one entry under its own wire
/// id; a SpawnWithComponents bundle expands to its Spawn plus one Insert
/// per kind, all under the bundle's id — mirroring the client's split.
fn fold_command_evidence(
    last_journaled: &mut Option<CommandId>,
    written: &[(CommandId, EntityCommand)],
    tick: Tick,
    out: &mut Vec<CommandEvidence>,
) {
    for (command_id, command) in written {
        // Ada eighth read: first write wins. A resend after a late ack
        // carries the same wire id and journals nothing new; the whole
        // command — including a bundle's synthetic Inserts — is skipped.
        if !journal_first_write(last_journaled, *command_id) {
            continue;
        }
        if let Some((kind, entity, component_kind)) = command_evidence_kind(command) {
            out.push((kind, entity, component_kind, tick, Some(*command_id)));
        }
        if let EntityCommand::SpawnWithComponents(entity, kinds) = command {
            for kind in kinds {
                out.push((
                    CauseKind::Insert,
                    *entity,
                    Some(*kind),
                    tick,
                    Some(*command_id),
                ));
            }
        }
    }
}

/// Ada eighth read (resend identity): the server journals a command on its
/// first write only. Returns true when `id` is newer than the observer's
/// last journaled mark (u16 wrap aware) and advances the mark; a resend of
/// an already-journaled id, or a stale id from behind the mark, returns
/// false without moving it.
pub(crate) fn journal_first_write(last: &mut Option<CommandId>, id: CommandId) -> bool {
    match *last {
        None => {
            *last = Some(id);
            true
        }
        Some(mark) => {
            let ahead = id.wrapping_sub(mark);
            if ahead == 0 || ahead >= 32768 {
                false
            } else {
                *last = Some(id);
                true
            }
        }
    }
}

/// nydus-dm9 Fix2: the applied-cause kind for one written entity command,
/// under the same identity the client journals for the same wire message.
/// Lifecycle commands and scope transitions are evidence; delegation and
/// authority traffic is admin that neither side journals.
/// `SpawnWithComponents` maps to `Spawn` here; the per-kind `Insert` entries
/// its bundle expands to are added by the caller, mirroring the client's
/// channel split (`remote_entity_channel` emits one synthetic `Spawn` plus
/// one `InsertComponent` per kind, all at the bundle's tick).
fn command_evidence_kind(
    command: &EntityCommand,
) -> Option<(CauseKind, GlobalEntity, Option<ComponentKind>)> {
    match command {
        EntityCommand::Spawn(entity) => Some((CauseKind::Spawn, *entity, None)),
        EntityCommand::SpawnWithComponents(entity, _) => Some((CauseKind::Spawn, *entity, None)),
        EntityCommand::InsertComponent(entity, kind) => {
            Some((CauseKind::Insert, *entity, Some(*kind)))
        }
        EntityCommand::RemoveComponent(entity, kind) => {
            Some((CauseKind::Remove, *entity, Some(*kind)))
        }
        EntityCommand::Despawn(entity) => Some((CauseKind::Despawn, *entity, None)),
        EntityCommand::Publish(_, entity) => Some((CauseKind::ScopePublish, *entity, None)),
        EntityCommand::Unpublish(_, entity) => Some((CauseKind::ScopeUnpublish, *entity, None)),
        EntityCommand::EnableDelegation(..)
        | EntityCommand::DisableDelegation(..)
        | EntityCommand::SetAuthority(..)
        | EntityCommand::RequestAuthority(..)
        | EntityCommand::ReleaseAuthority(..)
        | EntityCommand::EnableDelegationResponse(..)
        | EntityCommand::MigrateResponse(..) => None,
    }
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
            last_data_tick: None,
            last_journaled_command_id: None,
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
        let mut base_notifiables: [&mut dyn PacketNotifiable; 2] = [message_manager, world_manager];
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
        self.base
            .write_header_with(packet_type, last_rx, ack_bits, writer)
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
    #[expect(
        clippy::too_many_arguments,
        reason = "the transport operation carries independently typed protocol and world state"
    )]
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
                incoming_events.push_message(user_key, &channel_kind, message);
            }
        }

        // Receive Request and Response Events
        let (requests, responses) = self.base.message_manager.receive_requests_and_responses();
        for (channel_kind, requests) in requests {
            for (local_response_id, wire_nonce, request) in requests {
                let global_response_id = global_response_manager.create_response_id(
                    user_key,
                    &channel_kind,
                    local_response_id,
                    wire_nonce,
                );
                incoming_events.push_request(user_key, &channel_kind, global_response_id, request);
            }
        }
        for (global_request_id, response) in responses {
            global_request_manager.receive_response(global_request_id, response);
        }

        // Receive World Events
        if client_authoritative_entities {
            // Server game events never feed the applied-cause journal: the
            // wire ids stay with the remote manager's own return, and only
            // the events flow on.
            self.base
                .world_manager
                .take_incoming_events(
                    global_entity_map,
                    global_world_manager,
                    component_kinds,
                    world,
                    now,
                )
                .into_iter()
                .map(|(_, event)| event)
                .collect()
        } else {
            Vec::new()
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the transport operation carries independently typed protocol and world state"
    )]
    #[expect(
        clippy::expect_used,
        reason = "a poisoned GlobalDiffHandler lock means a writer panicked mid-update; replicating from it would corrupt every peer"
    )]
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
    ) -> (BitWriter, Vec<CommandEvidence>) {
        let next_packet_index = self.base.next_packet_index();

        let mut writer = BitWriter::new();
        writer.reserve_bits(3);

        self.write_header(PacketType::Data, &mut writer);

        let tick = time_manager.current_tick();
        tick.ser(&mut writer);
        time_manager.current_tick_instant().ser(&mut writer);
        self.last_data_tick = Some(tick);

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
        let diff_handler_guard = diff_handler_arc
            .read()
            .expect("GlobalDiffHandler lock poisoned");
        #[cfg(feature = "bench_instrumentation")]
        let t_base_write = std::time::Instant::now();
        let mut written_commands = Vec::new();
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
            Some(&mut written_commands),
        );
        #[cfg(feature = "bench_instrumentation")]
        bench_send_counters::NS_WRITE_UPDATES.fetch_add(
            t_base_write.elapsed().as_nanos() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );

        // nydus-dm9 Fix2 + Ada eighth read: journal every command this
        // packet consumed, stamped with the packet's own tick — the same
        // tick the client reads off the wire and journals under — and with
        // the command's own wire id. First write wins: a resend after a
        // late ack carries the same id and journals nothing new.
        let mut command_evidence = Vec::new();
        fold_command_evidence(
            &mut self.last_journaled_command_id,
            &written_commands,
            tick,
            &mut command_evidence,
        );

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

        (writer, command_evidence)
    }

    /// IO-free variant of the transmit path. Builds all outgoing packets for
    /// this connection without sending them. Returns `(packets, any_built)`.
    /// This is the sole server send path: every server packet is built here,
    /// so command evidence is always journaled (see `transmit_send_job`).
    /// `rtt_millis` is supplied by the caller (see `transmit_send_job`).
    ///
    /// # Why the packet count is bounded per pass
    ///
    /// Bandwidth is not the only ceiling — the ACK CHANNEL is. A remote header can
    /// acknowledge at most `REDUNDANT_PACKET_ACKS_SIZE + 1` (33) packets: one
    /// explicit index plus a 32-bit bitfield of its predecessors
    /// (`naia_shared::connection::ack_manager`). A client sends on the order of one
    /// packet per tick, so beyond ~33 packets per tick the ack window slides past the
    /// excess and those packets are NEVER acknowledged. Since a reliable message is
    /// retired only by an ack, its send queue then stops draining, every entry
    /// retransmits forever, and the added duplicates make it worse — congestion
    /// collapse whose cause is the ack window, not the link.
    ///
    /// Measured on the World Editor tile stream (localhost, 25 Hz, 2 MB/s budget,
    /// ~34 fragments per 64×64 raster): with only the bandwidth bound the send queue
    /// pinned at 991 of 1024 with 0 messages due, delivery stopped entirely, and
    /// 10,745 packet writes produced 1,070 deliveries. Bounding a pass to the ack
    /// window makes throughput ack-paced: the queue drains monotonically and nothing
    /// is orphaned. Deferred messages are not dropped — they stay queued and go out
    /// on the next pass, exactly like the bandwidth-deferred case.
    ///
    /// This is a pacing bound, not a wire change: no bytes are packed differently.
    #[expect(
        clippy::too_many_arguments,
        reason = "the transport operation carries independently typed protocol and world state"
    )]
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
    ) -> (Vec<OutgoingPacket>, bool, Vec<CommandEvidence>) {
        self.base.collect_messages(now, rtt_millis);
        let mut host_world_events = self
            .base
            .world_manager
            .take_outgoing_commands(now, rtt_millis);
        self.base.accumulate_bandwidth(now);

        let mut packets = Vec::new();
        let mut command_evidence = Vec::new();
        // Set by `build_one_packet` when it refuses on bandwidth rather than on an
        // empty queue. Both refusals return `None`, so without this the caller cannot
        // tell a bandwidth wall from a supply wall — see `send_pass_walls`.
        let mut refused_on_bandwidth = false;
        while packets.len() < MAX_PACKETS_PER_SEND_PASS {
            let Some((pkt, mut packet_evidence)) = self.build_one_packet(
                &mut refused_on_bandwidth,
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
            command_evidence.append(&mut packet_evidence);
        }

        // D6 L1 EMIT leg (Gerry 92237 / Usher 92247): every packet built
        // here is PacketType::Data. Marker-only; never gates behavior.
        self.shared.note_data_packets_built(packets.len() as u64);

        #[cfg(feature = "e2e_debug")]
        {
            use send_pass_walls::*;
            use std::sync::atomic::Ordering;
            if packets.len() >= MAX_PACKETS_PER_SEND_PASS {
                ACK_LIMITED.fetch_add(1, Ordering::Relaxed);
            } else if refused_on_bandwidth {
                BANDWIDTH_LIMITED.fetch_add(1, Ordering::Relaxed);
            } else {
                SUPPLY_LIMITED.fetch_add(1, Ordering::Relaxed);
            }
            MAX_PACKETS_IN_A_PASS.fetch_max(packets.len(), Ordering::Relaxed);
        }

        let any_built = !packets.is_empty();
        if any_built {
            self.base.mark_sent();
        }
        (packets, any_built, command_evidence)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the transport operation carries independently typed protocol and world state"
    )]
    fn build_one_packet<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        &mut self,
        refused_on_bandwidth: &mut bool,
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
    ) -> Option<(OutgoingPacket, Vec<CommandEvidence>)> {
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
            self.last_data_tick = Some(time_manager.current_tick());
            // Ack-only packets consume no commands: no evidence.
            return Some((writer.to_packet(), Vec::new()));
        }

        // Heartbeats update the clock estimate, but only Data advances the
        // client's receive ceiling. An idle world must still supply that tick.
        let needs_tick_boundary = self.last_data_tick != Some(time_manager.current_tick());
        if has_events || has_messages || needs_tick_boundary {
            if !self.base.can_spend_bandwidth(MTU_SIZE_BYTES as u32) {
                self.base.record_bandwidth_deferred();
                *refused_on_bandwidth = true;
                return None;
            }
            let (writer, command_evidence) = self.write_packet(
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
            // Packet length is bounded by MTU_SIZE_BYTES, always far below
            // u32::MAX, so this cast is lossless in practice.
            self.base.spend_bandwidth(packet.slice().len() as u32);
            return Some((packet, command_evidence));
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
            client_tick,
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

#[cfg(all(test, feature = "test_time"))]
mod quiet_data_tests {
    use std::time::Duration;

    use naia_shared::{
        BigMapKey, ChannelKinds, ComponentKinds, EntityAndGlobalEntityConverter,
        EntityDoesNotExistError, GameInstant, GlobalEntity, Instant, MessageKinds, PacketType,
        Serde, SnapshotMap, SnapshotWorld, StandardHeader, TestClock, Tick,
    };

    use super::SendConnection;
    use crate::{
        connection::connection::new_connection_pair, time_manager::TimeManager,
        world::global_world_manager::GlobalWorldManager, ServerConfig, UserKey,
    };

    struct Converter;

    impl EntityAndGlobalEntityConverter<u64> for Converter {
        fn global_entity_to_entity(
            &self,
            entity: GlobalEntity,
        ) -> Result<u64, EntityDoesNotExistError> {
            Ok(entity.to_u64())
        }

        fn entity_to_global_entity(
            &self,
            entity: &u64,
        ) -> Result<GlobalEntity, EntityDoesNotExistError> {
            Ok(GlobalEntity::from_u64(*entity))
        }
    }

    struct Link {
        connection: SendConnection,
        global_world_manager: GlobalWorldManager,
        time_manager: TimeManager,
        now: Instant,
    }

    impl Link {
        #[expect(
            clippy::unwrap_used,
            reason = "test helper: a failure must fail the calling test"
        )]
        fn new() -> Self {
            TestClock::init(0);
            let config = ServerConfig::default();
            let global_world_manager = GlobalWorldManager::new();
            let (_, mut connection) = new_connection_pair(
                &config.connection,
                &config.ping,
                &"127.0.0.1:54501".parse().unwrap(),
                UserKey::from_u64(1),
                &ChannelKinds::new(),
                &global_world_manager,
                32,
            );
            let time_manager = TimeManager::new(Duration::from_millis(40));
            let now = Instant::now();
            connection.base.accumulate_bandwidth(&now);
            Self {
                connection,
                global_world_manager,
                time_manager,
                now,
            }
        }

        fn advance(&mut self) {
            TestClock::advance(40);
            self.now = Instant::now();
            assert!(self.time_manager.recv_server_tick(&self.now));
        }

        fn build(&mut self) -> Vec<naia_shared::OutgoingPacket> {
            self.connection
                .build_all_packets(
                    &ChannelKinds::new(),
                    &MessageKinds::new(),
                    &ComponentKinds::new(),
                    &self.now,
                    &SnapshotWorld::<u64>::new(),
                    &Converter,
                    &self.global_world_manager,
                    &self.time_manager,
                    0.0,
                    &mut Vec::new(),
                    &SnapshotMap::new(),
                )
                .0
        }
    }

    #[expect(
        clippy::unwrap_used,
        reason = "test helper: a failure must fail the calling test"
    )]
    fn assert_empty_data(packet: &naia_shared::OutgoingPacket, tick: Tick) {
        let mut reader = naia_shared::BitReader::new(packet.slice());
        assert_eq!(
            StandardHeader::de(&mut reader).unwrap().packet_type,
            PacketType::Data
        );
        assert_eq!(Tick::de(&mut reader).unwrap(), tick);
        GameInstant::de(&mut reader).unwrap();
        for _ in 0..3 {
            assert!(!bool::de(&mut reader).unwrap());
        }
    }

    #[test]
    fn quiet_link_builds_one_data_boundary_per_server_tick() {
        let mut link = Link::new();
        for _ in 0..3 {
            link.advance();
            let packets = link.build();
            assert_eq!(
                packets.len(),
                1,
                "a quiet tick must still deliver its Data boundary"
            );
            assert_empty_data(&packets[0], link.time_manager.current_tick());
            assert!(
                link.build().is_empty(),
                "repeated sends in the same tick must stay bounded"
            );
        }
    }

    #[test]
    fn quiet_link_defers_its_data_boundary_when_bandwidth_is_exhausted() {
        let mut link = Link::new();
        link.connection.base.spend_bandwidth(10_000);
        link.advance();
        assert!(link.build().is_empty());
        for _ in 0..4 {
            link.advance();
        }
        let packets = link.build();
        assert_eq!(
            packets.len(),
            1,
            "the quiet boundary must resume when budget returns"
        );
        assert_empty_data(&packets[0], link.time_manager.current_tick());
    }

    #[test]
    fn quiet_link_ack_only_data_already_supplies_the_tick_boundary() {
        let mut link = Link::new();
        link.advance();
        link.connection.base.mark_should_send_empty_ack();
        let packets = link.build();
        assert_eq!(packets.len(), 1);
        assert_empty_data(&packets[0], link.time_manager.current_tick());
        assert!(link.build().is_empty());
        link.advance();
        assert_eq!(
            link.build().len(),
            1,
            "progress must not depend on another ACK request"
        );
    }
}

/// nydus-dm9 Fix2 (Ada 181408 point 2): the server must journal entity
/// actions and scope-channel evidence at the serialization point, under
/// the same identity the client records, or every lifecycle/scope event
/// is a client entry with no server match.
#[cfg(test)]
mod command_evidence_tests {
    use std::any::TypeId;

    use naia_shared::{
        BigMapKey, CauseKind, CommandId, ComponentKind, EntityCommand, GlobalEntity,
    };

    use super::command_evidence_kind;

    fn kind() -> ComponentKind {
        ComponentKind::from(TypeId::of::<u32>())
    }

    fn e(index: u64) -> GlobalEntity {
        GlobalEntity::from_u64(index)
    }

    /// One observer sees spawn, insert, update-protocol remove, despawn and
    /// both scope transitions: the written commands must map to the same
    /// kind sequence the client journals for the same wire messages.
    #[test]
    fn written_commands_map_to_matcher_kinds() {
        let written = [
            EntityCommand::Spawn(e(1)),
            EntityCommand::InsertComponent(e(1), kind()),
            EntityCommand::RemoveComponent(e(1), kind()),
            EntityCommand::Despawn(e(1)),
            EntityCommand::Publish(None, e(2)),
            EntityCommand::Unpublish(None, e(2)),
        ];
        let kinds: Vec<CauseKind> = written
            .iter()
            .filter_map(command_evidence_kind)
            .map(|(kind, _, _)| kind)
            .collect();
        assert_eq!(
            kinds,
            vec![
                CauseKind::Spawn,
                CauseKind::Insert,
                CauseKind::Remove,
                CauseKind::Despawn,
                CauseKind::ScopePublish,
                CauseKind::ScopeUnpublish,
            ],
        );
    }

    /// Delegation and authority traffic is not matcher evidence: neither
    /// side journals it as an applied cause.
    #[test]
    fn authority_commands_are_not_evidence() {
        let admin = [
            EntityCommand::EnableDelegation(None, e(1)),
            EntityCommand::DisableDelegation(None, e(1)),
            EntityCommand::RequestAuthority(None, e(1)),
            EntityCommand::ReleaseAuthority(None, e(1)),
        ];
        assert!(admin
            .iter()
            .filter_map(command_evidence_kind)
            .next()
            .is_none());
    }

    /// Ada eighth read, test (a) at the fold level: the same command id
    /// written in two packets — a resend after a late ack — journals one
    /// entry, keeping the first-write tick and the command's own id.
    #[test]
    fn fold_journals_a_resent_command_once() {
        use super::fold_command_evidence;
        let mut last: Option<CommandId> = None;
        let mut out = Vec::new();
        let written = [(11u16, EntityCommand::Spawn(e(1)))];
        fold_command_evidence(&mut last, &written, 5, &mut out);
        fold_command_evidence(&mut last, &written, 9, &mut out);
        assert_eq!(
            out.len(),
            1,
            "a resend after a late ack is the same cause, not a new entry: {out:?}",
        );
        assert_eq!(out[0].3, 5, "the entry keeps the first-write tick");
        assert_eq!(
            out[0].4,
            Some(11),
            "the entry carries the command's wire id"
        );
    }

    /// Ada eighth read (resend identity): the server journals a command on
    /// its first write only. A resend after a late ack carries the same
    /// wire id, so it must not journal a second entry.
    #[test]
    fn resend_after_late_ack_journals_once() {
        use super::journal_first_write;
        let mut last: Option<CommandId> = None;
        assert!(
            journal_first_write(&mut last, 11),
            "first write of id 11 journals",
        );
        assert_eq!(last, Some(11));
        assert!(
            !journal_first_write(&mut last, 11),
            "resend of id 11 after a late ack skips: same id, same cause",
        );
        assert_eq!(last, Some(11), "a skipped resend moves the mark nowhere");
        assert!(
            journal_first_write(&mut last, 12),
            "the next new id journals and advances the mark",
        );
        assert_eq!(last, Some(12));
    }

    /// A stale resend from before the last journaled id is not a new cause
    /// either: ids only move forward per observer (u16 wrap aware).
    #[test]
    fn stale_resend_behind_the_mark_skips() {
        use super::journal_first_write;
        let mut last: Option<CommandId> = None;
        assert!(journal_first_write(&mut last, 12));
        assert!(
            !journal_first_write(&mut last, 11),
            "id 11 behind the mark is an old cause, not a new one",
        );
    }

    /// The mark walks across the u16 wrap: 65535 then 0 is forward, and the
    /// pre-wrap ids never journal again.
    #[test]
    fn wrap_around_is_forward() {
        use super::journal_first_write;
        let mut last: Option<CommandId> = None;
        assert!(journal_first_write(&mut last, 65534));
        assert!(journal_first_write(&mut last, 65535));
        assert!(
            journal_first_write(&mut last, 0),
            "u16 wrap is forward, not behind",
        );
        assert_eq!(last, Some(0));
        assert!(
            !journal_first_write(&mut last, 65535),
            "the pre-wrap id is now stale",
        );
        assert!(
            !journal_first_write(&mut last, 65534),
            "older pre-wrap ids stay stale",
        );
    }
}
