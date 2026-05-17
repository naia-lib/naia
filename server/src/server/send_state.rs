//! Send-thread state owned by the pipeline coordinator (step 4-E).
//!
//! After `WorldServer::into_pipeline_states()` consumes the WorldServer,
//! `SendState<E>` carries every field the send thread needs:
//! `send_user_connections` (the send halves of every connection), the
//! per-user priority layer, the outbound `PacketSender`, and a clone of
//! `Arc<ServerShared<E>>` for lock-free access to init-only config and
//! per-connection cross-thread atomics.
//!
//! `SendHandle<E>` owns a `SendState<E>` directly (not via `Arc<Mutex>`)
//! so the send thread runs without contending with the recv or
//! coordinator threads for connection state.

use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    net::SocketAddr,
    sync::Arc,
};

use log::warn;

use naia_shared::{
    BitWriter, GlobalPriorityState, OwnedBitReader, PacketType, Serde, Tick, Timer,
    UserPriorityState,
};

use crate::{
    connection::{io::SendIo, RecvConnection, SendConnection},
    server::ServerShared,
    user::UserKey,
};

/// Send-thread-exclusive state lifted out of `WorldServer` (step 4-E).
pub struct SendState<E: Copy + Eq + Hash + Send + Sync> {
    /// Per-address map of send-side connection halves. Each holds a clone
    /// of the matching connection's `Arc<ConnectionShared>` for ack/RTT
    /// reads.
    pub send_user_connections: HashMap<SocketAddr, SendConnection>,

    /// Per-user priority layer. Entries evicted on scope exit; whole
    /// map entry dropped on disconnect (handled by the coordinator).
    pub user_priorities: HashMap<UserKey, UserPriorityState<E>>,

    /// Sender-wide priority layer (step 4-E.2e). Authoritative for the
    /// Iris send-path read. Kept in sync with `coord.global_priority_mirror`
    /// via publish-on-read at the top of `send_all_packets` — the borrow
    /// API `global_entity_priority_mut` would need a public-API change in
    /// `naia-shared::EntityPriorityMut` to push per-entity updates through
    /// the `SendStateUpdate` queue. A later commit can rewire that path
    /// (the `SendStateUpdate::PriorityChanged` variant is already defined).
    pub global_priority: GlobalPriorityState<E>,

    /// Periodic heartbeat send cadence (relocated from `RecvState` in
    /// 4-F.naia.c.2a). Fires when `handle_heartbeats` should sweep every
    /// `send_user_connection` for an outbound heartbeat packet — entirely
    /// send-side state, so it lives here now that the send half drives
    /// the dispatch loop.
    pub(crate) heartbeat_timer: Timer,

    /// Periodic ping send cadence (relocated from `RecvState` in
    /// 4-F.naia.c.2c). Send-side because the dispatch (`write_header` +
    /// `send_io.send_packet` + `mark_sent`) is send-side; only the
    /// per-user `ping_manager.write_ping(...)` read crosses into the
    /// recv half, and the coord/serial caller passes a `&mut recv_conns`
    /// borrow for that.
    pub(crate) ping_timer: Timer,

    /// Send half of the transport (step 4-E.2a). Carries the encoder,
    /// outgoing bandwidth monitor, and per-tick byte counter alongside
    /// the `Box<dyn PacketSender>`. Owned here so the send thread has
    /// exclusive mutable access without locking.
    pub send_io: SendIo,

    /// Shared init-only config + cross-thread atomic cells.
    pub shared: Arc<ServerShared<E>>,
}

impl<E: Copy + Eq + Hash + Send + Sync> SendState<E> {
    /// Periodic ping dispatch (step 4-F.naia.c.2c — relocated from
    /// `WorldServer::handle_pings`).
    ///
    /// Send-side because every operation other than
    /// `recv_conn.ping_manager.write_ping(...)` is send-side: the
    /// `ping_timer` lives on `SendState`, the per-user header is
    /// assembled via `send_conn.write_header` (reads the cross-half ack
    /// snapshot atomic), the packet is dispatched via `self.send_io`,
    /// and `mark_sent` updates the send-side heartbeat suppression. The
    /// `&mut recv_conns` borrow exists solely so each user's
    /// `ping_manager.should_send_ping()` / `write_ping(...)` can run
    /// against the recv-half body data.
    ///
    /// Serial mode: called from `WorldServer::receive_all_packets` with
    /// `&mut self.recv.recv_user_connections`. Pipeline mode: called by
    /// the coordinator with `&mut recv_handle.state.recv_user_connections`
    /// either standalone or alongside `process_recv_packets`.
    pub fn send_pings(
        &mut self,
        recv_conns: &mut HashMap<SocketAddr, RecvConnection>,
    ) {
        if !self.ping_timer.ringing() {
            return;
        }
        self.ping_timer.reset();

        let tm_guard = self.shared.time_manager.read();
        let tm = &*tm_guard;
        for (user_address, recv_conn) in recv_conns.iter_mut() {
            if !recv_conn.ping_manager.should_send_ping() {
                continue;
            }
            let Some(send_conn) = self.send_user_connections.get_mut(user_address) else {
                continue;
            };
            let mut writer = BitWriter::new();

            // write header (send side, reads ack snapshot atomic)
            let _header = send_conn.write_header(PacketType::Ping, &mut writer);

            // write server tick
            tm.current_tick().ser(&mut writer);
            tm.current_tick_instant().ser(&mut writer);

            // write body (recv side)
            recv_conn.ping_manager.write_ping(&mut writer, tm);

            if self
                .send_io
                .send_packet(user_address, writer.to_packet())
                .is_err()
            {
                // Ping send failure is not fatal: the connection timeout
                // will detect a persistently dead link via missed pongs.
                warn!("Server Error: Cannot send ping packet to {}", user_address);
            }
            send_conn.base.mark_sent();
        }
    }

    /// Coordinator-stage cross-half processing (step 4-F.naia.c.2b).
    ///
    /// Runs the work that used to live at the tail of
    /// `WorldServer::receive_all_packets` and inside
    /// `decode_pending_data_packets`. Takes a `&mut` borrow of the
    /// recv-side connection map so the tick-buffer decode can mutate
    /// `RecvConnection::tick_buffer` alongside the send-side ack /
    /// world-manager updates without breaking the recv/send state
    /// separation: in pipeline mode the coordinator can hand both halves
    /// in concurrently because both threads are paused on the coord-stage
    /// handoff.
    ///
    /// Sequence (in order):
    /// 1. Per-address `send_conn.drain_acks(&mut [])` over every
    ///    `received_addresses` entry — replaces the per-packet drains
    ///    that used to fire inline in the Heartbeat / Ping / Pong recv
    ///    handlers. The `AckSample` channel is crossbeam-backed and FIFO,
    ///    so coalescing per-packet drains into one per-address drain
    ///    preserves delivery-notification order within each connection
    ///    (see 4-F.naia.c.1 as-landed lesson #6 — the broken draft
    ///    consolidated *across* addresses; this method does not).
    /// 2. Per-data-packet tick-buffer decode + message/world decode +
    ///    arm-empty-ack. Drain is intentionally NOT repeated here — step
    ///    1 already drained every address that sent at least one packet
    ///    (Data-only addresses are a subset of `received_addresses`).
    /// 3. Per-address `send_conn.process_received_commands()` over every
    ///    `received_addresses` entry — finalizes any commands the decode
    ///    deposited into `send_conn.base.world_manager`.
    pub fn process_recv_packets(
        &mut self,
        recv_conns: &mut HashMap<SocketAddr, RecvConnection>,
        received_addresses: HashSet<SocketAddr>,
        pending_data_packets: Vec<(SocketAddr, Tick, OwnedBitReader)>,
        server_tick: Tick,
    ) {
        // Step 1: per-address ack drain.
        for address in &received_addresses {
            if let Some(send_conn) = self.send_user_connections.get_mut(address) {
                send_conn.drain_acks(&mut []);
            }
        }

        // Step 2: per-data-packet decode.
        for (address, client_tick, owned_reader) in pending_data_packets {
            let mut reader = owned_reader.borrow();

            let Some(send_conn) = self.send_user_connections.get_mut(&address) else {
                continue;
            };
            let Some(recv_conn) = recv_conns.get_mut(&address) else {
                continue;
            };

            // Recv-side: decode tick-buffered messages.
            if recv_conn
                .tick_buffer
                .read_messages(
                    &self.shared.channel_kinds,
                    &self.shared.message_kinds,
                    &server_tick,
                    &client_tick,
                    send_conn.base.world_manager.entity_converter(),
                    &mut reader,
                )
                .is_err()
            {
                warn!(
                    "Server Error: cannot decode tick-buffered messages from {}",
                    address
                );
                continue;
            }

            // Send-side: decode message/world section.
            if send_conn
                .read_data_section(
                    &self.shared.channel_kinds,
                    &self.shared.message_kinds,
                    &self.shared.component_kinds,
                    self.shared.client_authoritative_entities,
                    client_tick,
                    &mut reader,
                )
                .is_err()
            {
                warn!(
                    "Server Error: cannot decode data section from {}",
                    address
                );
                continue;
            }

            // Arm an ACK-only response (heartbeat-style) so the client
            // gets immediate acknowledgement of this data packet even if
            // the server has nothing to send back this tick.
            send_conn.base.mark_should_send_empty_ack();
        }

        // Step 3: per-address command finalization.
        for address in received_addresses {
            if let Some(send_conn) = self.send_user_connections.get_mut(&address) {
                send_conn.process_received_commands();
            }
        }
    }
}

// SAFETY: PacketSender is a trait object; concrete impls used by naia
// (UDP and local transports) are Send. The HashMap fields are owned
// outright. UserPriorityState contains POD numeric state.
unsafe impl<E: Copy + Eq + Hash + Send + Sync> Send for SendState<E> {}
