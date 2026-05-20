//! Pipeline-mode handles for the recv and send halves of a `WorldServer`
//! (step 4-E.2f).
//!
//! Each handle owns its substate directly — no `Arc<Mutex<WorldServer>>`
//! wrapper. `RecvState<E>` and `SendState<E>` are both already `Send`
//! (their `unsafe impl Send` blocks at `recv_state.rs:96` and
//! `send_state.rs:48` carry the safety story), so the handles inherit
//! `Send` without any further unsafe.
//!
//! `WorldServer::into_pipeline_handles(self)` decomposes the server into
//! three pieces — `CoordinatorState<E>`, `RecvHandle<E>`, `SendHandle<E>` —
//! and `WorldServer::from_pipeline_states(...)` reassembles them. The
//! split is structural; thread-independent operation of the recv and
//! send halves is wired up by the bevy pipeline coordinator in step 4-F.
//!
//! # Status (2026-05-16, post cyberlith.d)
//!
//! The cyberlith side `GameCell::update` is in serial-equivalent mode
//! (cyberlith `2da625e1`) and currently does NOT call
//! `into_pipeline_handles` — it keeps `WorldServer` reassembled and
//! drives `receive_with_world` + `send_all_packets` inline. The
//! multi-thread pipeline (step 4-F.cyberlith.e) is the next milestone.
//!
//! # Architectural constraints relevant to step 4-F.cyberlith.e
//!
//! **C1 (cross-half access).** `SendHandle::process_recv_packets` below
//! takes `&mut recv_conns` (from `RecvState::recv_user_connections`).
//! In a fully-3-threaded design those connections live on the recv
//! thread and `SendState` lives in coord/send territory — no single
//! thread holds both, so this method cannot be called in true 3-thread
//! mode. The .e MVP keeps recv on the coord (single thread holds both
//! halves) and parallelises only sim + send.
//!
//! **C2 (World mutation).** The decoded entity events (spawn / insert /
//! despawn) are applied to `&mut World` by
//! `WorldServer::process_all_packets`, which mutates `self.send.*`
//! while doing so. After `into_pipeline_handles`, the coord no longer
//! holds a unified `WorldServer`. The .e MVP works around this by NOT
//! calling `into_pipeline_handles` — it uses `receive_with_world`
//! (cyberlith.d) which bundles `receive_all_packets +
//! process_all_packets` inline.
//!
//! **C3 (send extraction).** `SendHandle::send_all_packets` does not
//! exist yet. Adding it is step 4-F.naia.h — see the doc-comment on
//! `WorldServer::send_all_packets` for the three-phase factoring plan.
//!
//! See `cyberlith/_AGENTS/MISSION_CAPACITY_UPLIFT.md` ("4-F.cyberlith.e
//! — multi-thread pipeline coordinator", architectural reality check
//! C1/C2/C3/C4) for the design decisions these constraints drove.

use std::{collections::HashMap, hash::Hash, net::SocketAddr};

use naia_shared::{Instant, Tick, WorldRefType};

use crate::{
    connection::RecvConnection,
    room::RoomKey,
    server::{receive_output::ReceiveOutput, recv_state::RecvState, send_state::SendState},
    user::UserKey,
};

/// Recv-thread handle. Owns `RecvState<E>` directly.
///
/// In 4-F this handle is moved onto the recv thread; the coordinator
/// keeps `CoordinatorState<E>` and the `Arc<ServerShared<E>>` so the
/// three pieces can run concurrently behind the coordinator-driven
/// 12-step tick sequence (§8 line 1235 onwards).
pub struct RecvHandle<E: Copy + Eq + Hash + Send + Sync> {
    /// Recv-thread-exclusive substate (timers, queues, recv-side
    /// connection halves, pending data packets, recv io).
    pub state: RecvState<E>,
}

impl<E: Copy + Eq + Hash + Send + Sync> RecvHandle<E> {
    /// Consume this handle and return the inner `RecvState<E>` —
    /// used by `WorldServer::from_pipeline_states` for serial-mode
    /// reassembly until 4-F's coordinator runs the recv thread directly.
    pub fn into_state(self) -> RecvState<E> {
        self.state
    }

    /// Pipeline-mode recv step (step 4-F.naia.c.2b).
    ///
    /// Drives the recv-only socket loop (no `SendState` access) and
    /// packages the per-tick handoff queues into a [`ReceiveOutput`] for
    /// the coordinator to thread into:
    ///   1. `WorldServer::drain_pending_handshakes` (coord-stage; needs
    ///      `coord.user_store`).
    ///   2. `SendHandle::process_recv_packets` (which consumes
    ///      `received_addresses` + `pending_data_packets` alongside a
    ///      `&mut` borrow of the recv connection map for the tick-buffer
    ///      decode + per-address ack drain + command finalization).
    pub fn receive(&mut self) -> ReceiveOutput<E> {
        self.state.receive();
        let world_events = self.state.take_world_events();
        let mut tick_events = self.state.take_tick_events(&Instant::now());
        let pending_ticks: Vec<Tick> =
            tick_events.read::<crate::events::TickEvent>().collect();
        let received_addresses = std::mem::take(&mut self.state.received_addresses);
        let pending_data_packets = std::mem::take(&mut self.state.pending_data_packets);
        ReceiveOutput {
            world_events,
            pending_ticks,
            received_addresses,
            pending_data_packets,
        }
    }

    /// Drain the tick-buffered messages received for `tick` from every
    /// connected user.
    ///
    /// Recv-only — touches only [`RecvState::recv_user_connections`].
    /// Mirrors `WorldServer::receive_tick_buffer_messages` (`world_server.rs:825`),
    /// whose body reads exclusively from `self.recv.recv_user_connections`.
    /// Exposed on [`RecvHandle`] so cyberlith's per-tick decoder (moving
    /// off main onto Recv per MISSION_SIM_OWNS_WORLD D.5e.1+.2) can run
    /// without WorldServer reassembly.
    pub fn receive_tick_buffer_messages(
        &mut self,
        tick: &Tick,
    ) -> crate::connection::tick_buffer_messages::TickBufferMessages {
        let mut tick_buffer_messages =
            crate::connection::tick_buffer_messages::TickBufferMessages::new();
        for (_user_address, recv_conn) in self.state.recv_user_connections.iter_mut() {
            recv_conn.tick_buffer_messages(tick, &mut tick_buffer_messages);
        }
        tick_buffer_messages
    }

    /// Test-only: manufacture a recv connection for `(address, user_key)`
    /// (mirroring `WorldServer::finalize_connection`) and inject one
    /// tick-buffered message into it, so a test can exercise
    /// [`Self::receive_tick_buffer_messages`] without a full client
    /// handshake. Returns `true` if the message was accepted by the
    /// tick buffer.
    ///
    /// Used by `Plugin::sim_integration_full`'s D.3a park-window
    /// tick-buffer drain test, which cannot complete a real handshake
    /// because the pipeline's auth / `receive_user` connection-lifecycle
    /// layer is a separate (D.3b) concern.
    #[cfg(feature = "test_utils")]
    #[doc(hidden)]
    pub fn inject_tick_buffer_message_for_test<C: naia_shared::Channel, M: naia_shared::Message>(
        &mut self,
        address: std::net::SocketAddr,
        user_key: &crate::user::UserKey,
        host_tick: &Tick,
        message_tick: &Tick,
        message: M,
    ) -> bool {
        use crate::connection::connection::new_connection_pair;

        if !self.state.recv_user_connections.contains_key(&address) {
            let gwm_guard = self.state.shared.global_world_manager.read();
            let (recv_conn, _send_conn) = new_connection_pair(
                &self.state.shared.server_config.connection,
                &self.state.shared.server_config.ping,
                &address,
                user_key,
                &self.state.shared.channel_kinds,
                &*gwm_guard,
                self.state.shared.server_config.max_replicated_entities as usize,
            );
            drop(gwm_guard);
            self.state.recv_user_connections.insert(address, recv_conn);
        }

        let channel_kind = naia_shared::ChannelKind::of::<C>();
        let container = naia_shared::MessageContainer::new(Box::new(message));
        self.state
            .recv_user_connections
            .get_mut(&address)
            .expect("connection just inserted")
            .inject_tick_buffer_message(&channel_kind, host_tick, message_tick, container)
    }
}

/// Send-thread handle. Owns `SendState<E>` directly.
pub struct SendHandle<E: Copy + Eq + Hash + Send + Sync> {
    /// Send-thread-exclusive substate (per-user send connections,
    /// per-user + global priority layers, send io).
    pub state: SendState<E>,
}

impl<E: Copy + Eq + Hash + Send + Sync> SendHandle<E> {
    /// Consume this handle and return the inner `SendState<E>` —
    /// used by `WorldServer::from_pipeline_states` for serial-mode
    /// reassembly until 4-F's coordinator runs the send thread directly.
    pub fn into_state(self) -> SendState<E> {
        self.state
    }

    /// Pipeline-mode periodic ping dispatch (step 4-F.naia.c.2c).
    /// Thin wrapper that forwards to [`SendState::send_pings`].
    pub fn send_pings(
        &mut self,
        recv_conns: &mut HashMap<SocketAddr, RecvConnection>,
    ) {
        self.state.send_pings(recv_conns);
    }

    /// Pipeline-mode coord-stage cross-half processing (step 4-F.naia.c.2b).
    ///
    /// Thin wrapper that forwards to [`SendState::process_recv_packets`].
    /// The coordinator pulls `received_addresses` + `pending_data_packets`
    /// off a [`ReceiveOutput`] returned by [`RecvHandle::receive`] and
    /// passes `&mut recv_handle.state.recv_user_connections` for the
    /// tick-buffer decode borrow.
    pub fn process_recv_packets(
        &mut self,
        recv_conns: &mut HashMap<SocketAddr, RecvConnection>,
        output: &mut ReceiveOutput<E>,
        server_tick: Tick,
    ) {
        let received_addresses = std::mem::take(&mut output.received_addresses);
        let pending_data_packets = std::mem::take(&mut output.pending_data_packets);
        self.state.process_recv_packets(
            recv_conns,
            received_addresses,
            pending_data_packets,
            server_tick,
        );
    }

    /// Pipeline-mode send-half tick body (step 4-F.naia.h).
    ///
    /// Thin wrapper that forwards to [`SendState::send_all_packets`].
    /// Called by the 4-F.cyberlith.e coordinator on the send thread AFTER
    /// the coord thread has called `WorldServer::run_send_preamble` on
    /// the reassembled server (or, once the coord/send split is taken
    /// further, after a coord-side equivalent of the preamble runs).
    ///
    /// The world snapshot passed in must outlive the call. Today the
    /// only `WorldRefType + Sync` implementation that crosses thread
    /// boundaries is bevy's `world.proxy()`; if cyberlith.e finds it
    /// is not actually `Send`-safe to move across threads, the send
    /// thread will instead need a cheap clone of the relevant subset
    /// before this call. The signature here matches the serial
    /// `SendState::send_all_packets` shape exactly so the two paths
    /// remain interchangeable.
    pub fn send_all_packets<W: WorldRefType<E> + Sync>(&mut self, world: W) {
        self.state.send_all_packets(world);
    }

    /// C.6 prep — run the per-tick send preamble on `SendState` alone,
    /// without needing a world snapshot or a reassembled `WorldServer`.
    ///
    /// Cyberlith's Send SubApp calls this from its `ApplyExtract` phase
    /// (after extract delivers the inbound `SnapshotWorld<E>` from Sim,
    /// before serialization runs in `SerializePackets`). The subsequent
    /// `send_all_packets` invocation detects that the preamble already
    /// ran this tick and skips its inline preamble.
    ///
    /// See [`SendState::apply_pending_send_preamble`] for the full
    /// preamble sequence (room changes drain, outbound flush,
    /// heartbeats, empty acks).
    pub fn apply_pending_send_preamble(&mut self) {
        self.state.apply_pending_send_preamble();
    }

    /// C.6 prep #6 — drain entity-scope variants (`EntityEnteredRoom`,
    /// `UserEnteredRoom`, `UserLeftRoom`, `ScopeToggled`) from
    /// `scope_change_queue` and fan them out to user connections.
    ///
    /// Companion to [`apply_pending_send_preamble`] (which only drains
    /// `RoomChange` variants). Together the two methods restore the
    /// full body of the legacy `WorldServer::run_send_preamble` for
    /// pipeline-mode callers that hold a `SendHandle` directly.
    ///
    /// Cyberlith pattern:
    /// ```ignore
    /// send.apply_pending_send_preamble();
    /// send.apply_pending_scope_changes(&snap);
    /// send.send_all_packets(snap);
    /// ```
    ///
    /// The intermediate `apply_pending_scope_changes` is what publishes
    /// freshly-room-added entities into per-user `send_user_connections`
    /// (the spawn-to-user path). Without it, `send_all_packets`'s Iris
    /// dirty-scan finds no per-user spawn intents and clients see
    /// nothing for entities that entered rooms this tick.
    ///
    /// Needs a `WorldRefType<E>` because the per-(user, entity) fanout
    /// calls `world.has_entity(world_entity)` before spawning. Pass the
    /// same world snapshot you're about to feed `send_all_packets`.
    ///
    /// Idempotent within a tick: the per-tick flag causes the
    /// subsequent `send_all_packets` to skip its auto-call. See
    /// [`SendState::apply_pending_scope_changes`] for the full body.
    pub fn apply_pending_scope_changes<W: naia_shared::WorldRefType<E>>(
        &mut self,
        world: &W,
    ) {
        self.state.apply_pending_scope_changes(world);
    }

    /// MISSION_USER_ONLY_SEES_SIM Phase D.2.2 (2026-05-19) — drain the
    /// pending World-side `configure_entity_replication` hook ops onto
    /// `world`, installing per-component diff mutators against the
    /// (already-transitioned) gwm.
    ///
    /// Twin of [`crate::pipeline_actors::CoordHandle::apply_pending_world_hooks`]
    /// — both drain the same `shared.pending_world_hooks` queue, so a Sim
    /// system may call whichever handle it has in hand. The Send-side
    /// per-connection work for the same configure call drains separately
    /// inside [`apply_pending_send_preamble`]. Idempotent.
    pub fn apply_pending_world_hooks<W: naia_shared::WorldMutType<E>>(&self, world: &mut W)
    where
        E: 'static,
    {
        crate::server::send_state::drain_pending_world_hooks(&self.state.shared, world);
    }

    /// C.6 prep — send a message to the user at `address` without
    /// reassembling the WorldServer.
    ///
    /// Cyberlith pattern:
    /// ```ignore
    /// let addr = coord.user_address(user_key).expect("user exists");
    /// send.send_message_to_address::<EntityAssignmentChannel, _>(&addr, &msg);
    /// ```
    ///
    /// Internally builds the per-user `EntityAndGlobalEntityConverter`
    /// from `shared.global_world_manager` + the send connection's
    /// `world_manager`, so messages carrying `EntityProperty` fields
    /// resolve per-user local entity ids correctly.
    ///
    /// Returns `false` if no send connection exists at `address`
    /// (user disconnected between the address lookup and this call).
    pub fn send_message_to_address<C: naia_shared::Channel, M: naia_shared::Message>(
        &mut self,
        address: &std::net::SocketAddr,
        message: &M,
    ) -> bool {
        self.state.send_message_to_address::<C, M>(address, message)
    }

    /// MISSION_USER_ONLY_SEES_SIM Phase B.3 (2026-05-19) — convenience
    /// wrapper that resolves `user_key → SocketAddr` via the supplied
    /// `CoordHandle` and forwards to [`send_message_to_address`].
    ///
    /// Cyberlith pattern (no separate `coord.user_address` step):
    /// ```ignore
    /// send.send_message_to_user::<EntityAssignmentChannel, _>(&coord, user_key, &msg);
    /// ```
    ///
    /// Returns `false` if either:
    /// - the `user_key` is unknown to coord's `user_store` (user never
    ///   joined or was already removed), OR
    /// - no send connection exists at the resolved address (user
    ///   disconnected between the lookup and this call).
    ///
    /// Internally:
    /// 1. `coord.user_address(user_key)` → `Option<SocketAddr>`
    /// 2. If `None`, return `false`.
    /// 3. Otherwise forward to `send_message_to_address::<C, M>(&addr, message)`.
    ///
    /// Wire output is byte-identical to the legacy
    /// `Server::send_message(user_key, msg)` path (which internally does
    /// the same address resolution). The only behavioral difference is
    /// the `bool` return shape (vs. legacy's `Result<(), NaiaServerError>`),
    /// chosen for consistency with the sibling
    /// [`send_message_to_address`] entry point.
    pub fn send_message_to_user<C: naia_shared::Channel, M: naia_shared::Message>(
        &mut self,
        coord: &crate::pipeline_actors::CoordHandle<E>,
        user_key: &UserKey,
        message: &M,
    ) -> bool {
        let Some(address) = coord.user_address(user_key) else {
            return false;
        };
        self.send_message_to_address::<C, M>(&address, message)
    }

    // ============================================================
    // Phase A.3 — send-side scope APIs (cyberlith D5)
    // ============================================================
    //
    // The cyberlith Send SubApp computes its own per-user visibility (D5)
    // and needs to publish the result into naia's scope tables, which now
    // live on `SendState`. These helpers expose just-enough surface for
    // the send-side scope policy without re-introducing the
    // `WorldServer::user_scope_mut` chain that depends on
    // `CoordinatorState::user_store` and `RoomStore`.

    /// Returns a snapshot of the pending scope-check tuples
    /// `(room_key, user_key, world_entity)`.
    ///
    /// Mirrors `WorldServer::scope_checks_pending()` but operates against
    /// the relocated `SendState::scope_checks_cache`. Cyberlith's
    /// `run_scope_policy` system reads this each tick to decide which
    /// per-user / per-entity scope decisions to recompute.
    pub fn scope_checks_pending(&self) -> Vec<(RoomKey, UserKey, E)> {
        self.state.scope_checks_cache.pending_slice().to_vec()
    }

    /// Mark all currently-pending scope checks as handled.
    ///
    /// Mirrors `WorldServer::mark_scope_checks_pending_handled()`. Call
    /// after every batch returned by [`scope_checks_pending`].
    pub fn mark_scope_checks_pending_handled(&mut self) {
        self.state.scope_checks_cache.mark_pending_handled();
    }

    /// Set a per-user explicit scope bit for `global_entity`.
    ///
    /// This is the *raw* writer path: it inserts into
    /// `entity_scope_map` and pushes a `ScopeChange::ScopeToggled` onto
    /// the cross-half `scope_change_queue`. It does **NOT** run the
    /// publicity/owner pre-checks that
    /// `WorldServer::user_scope_set_entity` runs — D5's send-side scope
    /// policy is responsible for only invoking it on entities it has
    /// already determined are valid scope targets.
    ///
    /// `global_entity` is provided directly (not `world_entity`) so the
    /// call doesn't depend on the cyberlith-side world-entity ↔
    /// global-entity mapping (the Send SubApp can resolve via its
    /// `SendReplMap` if needed before calling).
    pub fn user_scope_set_global_entity(
        &mut self,
        user_key: &UserKey,
        global_entity: naia_shared::GlobalEntity,
        is_contained: bool,
    ) {
        self.state
            .entity_scope_map
            .insert(*user_key, global_entity, is_contained);
        self.state
            .shared
            .scope_change_queue
            .lock()
            .push_back(crate::server::scope_change::ScopeChange::ScopeToggled(
                *user_key,
                global_entity,
                is_contained,
            ));
    }

    /// Set a per-user explicit scope bit for `world_entity`.
    ///
    /// Convenience wrapper over [`user_scope_set_global_entity`]: looks up
    /// `world_entity → GlobalEntity` via the shared
    /// [`naia_shared::EntityAndGlobalEntityConverter`] (no scope mutation
    /// performed if the entity is not registered) and forwards to the
    /// global-entity-keyed setter.
    ///
    /// Mirrors `WorldServer::UserScopeMut::include / exclude` ergonomics
    /// for the Send SubApp world (where bevy entities are the natural
    /// key but the cross-half scope queue is keyed by `GlobalEntity`).
    /// Used by cyberlith D.5b's send-side `static_tile_scope_policy`.
    ///
    /// Returns `true` if a scope mutation was queued, `false` if
    /// `world_entity` was not registered with this server's
    /// `global_entity_map` (in which case the call is a no-op).
    pub fn user_scope_set_entity(
        &mut self,
        user_key: &UserKey,
        world_entity: &E,
        is_contained: bool,
    ) -> bool {
        use naia_shared::EntityAndGlobalEntityConverter;
        let global_entity = match self
            .state
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
        {
            Ok(ge) => ge,
            Err(_) => return false,
        };
        self.user_scope_set_global_entity(user_key, global_entity, is_contained);
        true
    }

    /// Phase A.3 deviation note: `room_mut` is NOT implemented on
    /// `SendHandle`. The spec listed it, but `RoomStore` stays on
    /// `CoordinatorState` (it's coord-thread state — user_store is its
    /// peer, and room membership is set via lifecycle events that arrive
    /// on the Recv SubApp). D5's send-side scope policy does not need
    /// it. Cyberlith reaches room state via `CoordHandle` on the Recv
    /// SubApp instead.
    #[doc(hidden)]
    fn _phase_a3_no_room_mut_on_send() {}
}
