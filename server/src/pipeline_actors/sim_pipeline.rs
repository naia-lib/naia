//! [`PipelinedServer<E>`] — unified pipeline handle; the primary entry point
//! for consumers of the pipelined server API.
//!
//! Construct with [`PipelinedServer::new`] (replaces `spawn_server_handles`).
//! Owns all three sub-handles:
//! - `coord` (coordination, main-thread only).
//! - `recv` (network-receive worker) — in a park-window slot shared with the worker.
//! - `send` (network-send worker) — in a park-window slot shared with the worker.
//!
//! # Primary API: `tick`
//!
//! ```ignore
//! server.tick(&mut world, |ctx| {
//!     ctx.host_sync();          // bevy-adapter extension
//!     ctx.send_all_packets();   // bevy-adapter extension
//! });
//! ```
//!
//! The body receives a [`TickCtx`] with owned `recv`/`send` handles and a
//! `&mut CoordHandle<E>` (`ctx.coord`) for the duration of the tick. Workers
//! must be parked before calling `tick()`; [`PipelinedServer`] does NOT own
//! the park/unpark machinery — that belongs to the adapter
//! (`PluginInternalState::park_workers`).
//!
//! # Worker slot sharing
//!
//! [`PipelinedServer::recv_slot`] and [`PipelinedServer::send_slot`] return
//! `Arc<Mutex<Option<...>>>` clones. The bevy adapter gives these to the
//! recv/send workers at spawn time; workers deposit their handle before
//! parking and re-claim it after unpark.

use std::{hash::Hash, net::SocketAddr, sync::Arc};

use parking_lot::Mutex;
use naia_shared::{EntityAuthStatus, Protocol, Tick, WorldMutType, WorldRefType};

use crate::{
    room::RoomKey, server::ServerShared, user::UserKey, EntityOwner, ReceiveOutput, RecvHandle,
    ReplicationConfig, SendHandle, ServerConfig, WorldServer,
};

use super::handles::CoordHandle;
use super::ServerEntityConverter;

// ── PipelinedServer ───────────────────────────────────────────────────────────

/// Unified pipeline handle. Construct with [`PipelinedServer::new`].
///
/// See module-level docs for the design overview.
pub struct PipelinedServer<E: Copy + Eq + Hash + Send + Sync> {
    /// Coordination handle (main-thread only). Stored as `Option` so the
    /// handle can be temporarily moved into a `WorldServer` for operations
    /// that require full-server access (e.g., `io_load` via [`Self::listen`]).
    pub(super) coord: Option<CoordHandle<E>>,
    /// Park-window slot shared with the recv worker.
    recv_slot: Arc<Mutex<Option<RecvHandle<E>>>>,
    /// Park-window slot shared with the send worker.
    send_slot: Arc<Mutex<Option<SendHandle<E>>>>,
}

impl<E: Copy + Eq + Hash + Send + Sync + 'static> PipelinedServer<E> {
    /// Construct a [`WorldServer<E>`] and immediately split it into a
    /// [`PipelinedServer<E>`].
    ///
    /// This is the primary entry point for consumers. Pass the result to the
    /// bevy adapter's `Plugin::pipelined` (via a `PipelinedServer` Bevy
    /// resource), then call [`Self::listen`] to bind a socket and drive ticks
    /// with [`Self::tick`].
    pub fn new<P: Into<Protocol>>(server_config: ServerConfig, protocol: P) -> Self {
        let ws = WorldServer::<E>::new(server_config, protocol);
        let (coord_state, recv, send) = ws.into_pipeline_handles();
        let shared: Arc<ServerShared<E>> = Arc::clone(&recv.state.shared);
        let coord = CoordHandle { state: coord_state, shared };
        Self::from_handles(coord, recv, send)
    }

    pub(super) fn from_handles(coord: CoordHandle<E>, recv: RecvHandle<E>, send: SendHandle<E>) -> Self {
        Self {
            coord: Some(coord),
            recv_slot: Arc::new(Mutex::new(Some(recv))),
            send_slot: Arc::new(Mutex::new(Some(send))),
        }
    }

    /// Borrow the coordination handle.
    ///
    /// Panics if `coord` is temporarily absent (only during [`Self::tick`] or
    /// [`Self::with_world_server`] — both restore it before returning).
    pub fn coord(&self) -> &CoordHandle<E> {
        self.coord.as_ref().expect("PipelinedServer: CoordHandle temporarily unavailable")
    }

    /// Mutably borrow the coordination handle.
    pub fn coord_mut(&mut self) -> &mut CoordHandle<E> {
        self.coord.as_mut().expect("PipelinedServer: CoordHandle temporarily unavailable")
    }

    /// `Arc` clone of the recv worker's park-window slot.
    ///
    /// The bevy adapter passes this to the recv worker at spawn time so the
    /// worker can deposit/re-claim its [`RecvHandle`] across park windows.
    pub fn recv_slot(&self) -> Arc<Mutex<Option<RecvHandle<E>>>> {
        Arc::clone(&self.recv_slot)
    }

    /// `Arc` clone of the send worker's park-window slot.
    ///
    /// Symmetric to [`Self::recv_slot`].
    pub fn send_slot(&self) -> Arc<Mutex<Option<SendHandle<E>>>> {
        Arc::clone(&self.send_slot)
    }

    /// Run one pipelined tick.
    ///
    /// Moves `recv` and `send` out of their slots, passes a [`TickCtx`] to
    /// `body`, then returns both handles to their slots. `coord` is also
    /// temporarily moved to allow framework-agnostic body implementations
    /// that need owned access.
    ///
    /// # Precondition
    ///
    /// Workers must be parked before calling `tick()` — this ensures the
    /// handles are in their slots. The bevy adapter wraps this with
    /// `PluginInternalState::park_workers` / `unpark_workers`.
    pub fn tick<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        body: impl FnOnce(&mut TickCtx<'_, E, W>),
    ) {
        let mut coord = self.coord.take().expect(
            "PipelinedServer::tick: CoordHandle not available — re-entrant tick?",
        );
        let recv = self.recv_slot.lock().take().expect(
            "PipelinedServer::tick: RecvHandle not in slot — park workers before calling tick()",
        );
        let send = self.send_slot.lock().take().expect(
            "PipelinedServer::tick: SendHandle not in slot — park workers before calling tick()",
        );
        let mut ctx = TickCtx {
            coord: &mut coord,
            recv,
            send,
            world,
        };
        body(&mut ctx);
        // Destructure to release the `coord` borrow (ctx.coord = &mut coord) before
        // moving coord back into the Option slot. The reference fields (coord, world)
        // are bound to `_` and dropped here, expiring their borrows.
        let TickCtx { coord: _, recv: recv_out, send: send_out, world: _ } = ctx;
        self.coord = Some(coord);
        *self.recv_slot.lock() = Some(recv_out);
        *self.send_slot.lock() = Some(send_out);
    }

    /// Take only the coordination handle, leaving recv/send slots untouched.
    ///
    /// Used by the bevy adapter's `drain_recv_impl_split` which already has
    /// independent access to the recv/send Arcs (via `RecvHandleRes` /
    /// `SendHandleRes`). MUST be followed by [`Self::restore_coord`] before the
    /// pipeline is used again.
    pub fn take_coord(&mut self) -> CoordHandle<E> {
        self.coord.take().expect("PipelinedServer::take_coord: CoordHandle not available")
    }

    /// Restore a coordination handle previously taken by [`Self::take_coord`].
    pub fn restore_coord(&mut self, coord: CoordHandle<E>) {
        self.coord = Some(coord);
    }

    /// Take all three handles for external manipulation.
    ///
    /// Workers must be parked before calling this. Returns `(coord, recv, send)`.
    /// MUST be followed by [`Self::restore_handles`] before workers are unparked
    /// or [`Self::tick`] is called. Used by the bevy adapter's `drain_recv_impl`
    /// which passes the handles through `apply_recv_to_world` (a loop that requires
    /// owned handle access and cannot use the `tick()` closure form).
    ///
    /// # Panic-poisoning caveat (audit #5)
    /// This empties all three slots; the paired `restore_handles` re-fills them.
    /// If a caller panics AFTER `take_handles` but BEFORE `restore_handles` (e.g.
    /// inside `apply_recv_to_world` or `transmit_send_job`), the slots stay empty
    /// and the NEXT entry panics with `"… not in slot — park workers first"` —
    /// which misdescribes the actual fault (a prior panic, not a missing park).
    /// `receive`/`send`/`tick` all share this hazard; it is acceptable for the
    /// oracle/test path (a panic there already aborts the run) but a debugger
    /// chasing the second panic should look upstream for the first.
    pub fn take_handles(&mut self) -> (CoordHandle<E>, RecvHandle<E>, SendHandle<E>) {
        let coord = self.coord.take().expect(
            "PipelinedServer::take_handles: CoordHandle not available",
        );
        let recv = self.recv_slot.lock().take().expect(
            "PipelinedServer::take_handles: RecvHandle not in slot — park workers first",
        );
        let send = self.send_slot.lock().take().expect(
            "PipelinedServer::take_handles: SendHandle not in slot — park workers first",
        );
        (coord, recv, send)
    }

    /// Restore handles previously taken by [`Self::take_handles`].
    pub fn restore_handles(&mut self, coord: CoordHandle<E>, recv: RecvHandle<E>, send: SendHandle<E>) {
        self.coord = Some(coord);
        *self.recv_slot.lock() = Some(recv);
        *self.send_slot.lock() = Some(send);
    }

    /// Bind a transport socket: splits the socket into its I/O handles and
    /// calls `io_load` on the reassembled `WorldServer`.
    ///
    /// This is the G2 startup-window entry point. Equivalent to:
    /// ```ignore
    /// let (_auth_tx, _auth_rx, ps, pr) = socket.into().listen();
    /// server.with_world_server(|ws| ws.io_load(ps, pr));
    /// ```
    ///
    /// Must be called while workers are not yet spawned (or parked). After
    /// this call the pipeline is ready for `tick()`.
    pub fn listen<S: Into<Box<dyn crate::transport::Socket>>>(&mut self, socket: S) {
        let (_auth_tx, _auth_rx, ps, pr) = crate::transport::Socket::listen(socket.into());
        self.with_world_server(|ws| ws.io_load(ps, pr));
    }

    /// Temporarily reassemble a [`crate::WorldServer`] and invoke `f` against it.
    ///
    /// All three handles are moved into the `WorldServer`; after `f` returns
    /// they are re-split and restored. Used for ops that require full-server
    /// access (e.g. `entity_replication_config` until G3 adds it to `CoordHandle`).
    ///
    /// Workers must be parked (or not yet started) before calling this.
    pub fn with_world_server<R>(
        &mut self,
        f: impl FnOnce(&mut crate::WorldServer<E>) -> R,
    ) -> R {
        let coord = self.coord.take().expect(
            "PipelinedServer::with_world_server: CoordHandle not available",
        );
        let recv = self.recv_slot.lock().take().expect(
            "PipelinedServer::with_world_server: RecvHandle not in slot",
        );
        let send = self.send_slot.lock().take().expect(
            "PipelinedServer::with_world_server: SendHandle not in slot",
        );

        let coord_state = coord.state;
        let mut ws = WorldServer::from_pipeline_states(coord_state, recv.state, send.state);
        let result = f(&mut ws);
        let (coord_state, recv_state, send_state) = ws.into_pipeline_states();
        let shared = Arc::clone(&recv_state.shared);

        self.coord = Some(CoordHandle { state: coord_state, shared });
        *self.recv_slot.lock() = Some(RecvHandle { state: recv_state });
        *self.send_slot.lock() = Some(SendHandle { state: send_state });

        result
    }
}

// ── G3a — coord-only op forwarding ──────────────────────────────────────────────
//
// MISSION_PIPELINE_API_BOUNDARY G3a (Connor 2026-06-29): every public
// coord-only op on [`CoordHandle<E>`] is mirrored here as a thin forwarding
// method so consumers operate on the unified [`PipelinedServer<E>`] handle
// directly — no `pipeline.coord_mut().method()` ceremony, no `WorldServer`
// reassembly. All methods delegate to `self.coord()` / `self.coord_mut()`,
// which panic only inside an in-flight `tick`/`with_world_server` window (the
// pipelined consumer calls these from the parked main thread, outside that
// window). `enable_entity_replication` is intentionally NOT forwarded here —
// it is superseded by the G5 `enable_replication_for_existing_entity` op.

impl<E: Copy + Eq + Hash + Send + Sync + 'static> PipelinedServer<E> {
    // ── Entity reads ──

    /// Forwards to [`CoordHandle::is_resource_entity`].
    pub fn is_resource_entity(&self, world_entity: &E) -> bool {
        self.coord().is_resource_entity(world_entity)
    }

    /// Forwards to [`CoordHandle::entity_owner`].
    pub fn entity_owner(&self, world_entity: &E) -> EntityOwner {
        self.coord().entity_owner(world_entity)
    }

    /// Forwards to [`CoordHandle::entity_authority_status`].
    pub fn entity_authority_status(&self, world_entity: &E) -> Option<EntityAuthStatus> {
        self.coord().entity_authority_status(world_entity)
    }

    /// Forwards to [`CoordHandle::entity_is_static`].
    pub fn entity_is_static(&self, world_entity: &E) -> bool {
        self.coord().entity_is_static(world_entity)
    }

    /// Forwards to [`CoordHandle::entity_converter`].
    pub fn entity_converter(&self) -> ServerEntityConverter<E> {
        self.coord().entity_converter()
    }

    // ── User ops ──

    /// Forwards to [`CoordHandle::user_exists`].
    pub fn user_exists(&self, user_key: &UserKey) -> bool {
        self.coord().user_exists(user_key)
    }

    /// Forwards to [`CoordHandle::user_keys`].
    pub fn user_keys(&self) -> Vec<UserKey> {
        self.coord().user_keys()
    }

    /// Forwards to [`CoordHandle::user_address`].
    pub fn user_address(&self, user_key: &UserKey) -> Option<SocketAddr> {
        self.coord().user_address(user_key)
    }

    /// Forwards to [`CoordHandle::receive_user`].
    pub fn receive_user(&mut self, user_key: UserKey, user_addr: SocketAddr) {
        self.coord_mut().receive_user(user_key, user_addr)
    }

    /// Forwards to [`CoordHandle::disconnect_user`].
    pub fn disconnect_user(&mut self, user_key: &UserKey) {
        self.coord_mut().disconnect_user(user_key)
    }

    // ── Room ops ──

    /// Forwards to [`CoordHandle::create_room`].
    pub fn create_room(&mut self) -> RoomKey {
        self.coord_mut().create_room()
    }

    /// Forwards to [`CoordHandle::room_destroy`].
    pub fn room_destroy(&mut self, room_key: &RoomKey) -> bool {
        self.coord_mut().room_destroy(room_key)
    }

    /// Forwards to [`CoordHandle::room_add_user`].
    pub fn room_add_user(&mut self, room_key: &RoomKey, user_key: &UserKey) {
        self.coord_mut().room_add_user(room_key, user_key)
    }

    /// Forwards to [`CoordHandle::room_remove_user`].
    pub fn room_remove_user(&mut self, room_key: &RoomKey, user_key: &UserKey) {
        self.coord_mut().room_remove_user(room_key, user_key)
    }

    /// Forwards to [`CoordHandle::room_add_entity`].
    pub fn room_add_entity(&mut self, room_key: &RoomKey, world_entity: &E) {
        self.coord_mut().room_add_entity(room_key, world_entity)
    }

    /// Forwards to [`CoordHandle::room_remove_entity`].
    pub fn room_remove_entity(&mut self, room_key: &RoomKey, world_entity: &E) {
        self.coord_mut().room_remove_entity(room_key, world_entity)
    }

    // ── Replication config ──

    /// Forwards to [`CoordHandle::mark_entity_as_static`].
    pub fn mark_entity_as_static(&mut self, world_entity: &E) {
        self.coord_mut().mark_entity_as_static(world_entity)
    }

    /// Forwards to [`CoordHandle::configure_entity_replication`].
    pub fn configure_entity_replication(&mut self, world_entity: &E, config: ReplicationConfig) {
        self.coord_mut()
            .configure_entity_replication(world_entity, config)
    }

    /// Forwards to [`CoordHandle::apply_pending_world_hooks`].
    pub fn apply_pending_world_hooks<W>(&self, world: &mut W)
    where
        W: WorldMutType<E>,
    {
        self.coord().apply_pending_world_hooks(world)
    }

    // ── Tick / queue introspection ──

    /// Forwards to [`CoordHandle::current_tick`].
    pub fn current_tick(&self) -> Tick {
        self.coord().current_tick()
    }

    /// Forwards to [`CoordHandle::scope_change_queue_len`].
    pub fn scope_change_queue_len(&self) -> usize {
        self.coord().scope_change_queue_len()
    }

    /// Forwards to [`CoordHandle::pending_world_hooks_len`].
    pub fn pending_world_hooks_len(&self) -> usize {
        self.coord().pending_world_hooks_len()
    }
}

// ── G7 — the `receive`/`send` bracket ───────────────────────────────────────────
//
// MISSION_PIPELINE_API_BOUNDARY G7 (Connor sign-off 2026-06-29): the
// framework-agnostic per-tick bracket. A consumer (bevy or non-bevy) drives one
// pipelined tick as an explicit method sequence — `receive` then its own
// interleaved gameplay/scope code, then `send` — with NO trait to implement, NO
// closures, NO hooks. This supersedes the closure-form `tick`/`with_parked_tick`.
//
// Both methods take/return the three handles via the park-window slots (same
// shape as `tick`). When the worker runtime is present (G7-3) it parks the
// workers around this bracket; when absent (the determinism/desync ORACLE, and
// every `naia_test_harness` drive) the bracket runs fully synchronously on the
// calling thread and emits bytes identical to worker-driven production by
// construction (G9pre §2i).

impl<E: Copy + Eq + Hash + Send + Sync + 'static> PipelinedServer<E> {
    /// **D0 — receive.** Park-window recv-drain + entity-op application.
    ///
    /// Drains the recv half once (`RecvHandle::receive`) and applies the decoded
    /// spawn/insert/update/despawn/handshake events to `world` via the core
    /// [`crate::pipeline_actors::apply_recv_to_world`] orchestration. Returns the
    /// resulting [`ReceiveOutput`] — the connection-scoped and entity-scoped
    /// EVENTS (Connect/Disconnect/Tick/Message/Request/Auth + Spawn/Despawn/…) —
    /// for the consumer (or its framework adapter) to fan out. Core mutates only
    /// `world` (the single entity world, §2h H3); it routes no events itself.
    ///
    /// `world` is taken **by value** (`W: WorldMutType<E>`), matching naia's
    /// world-proxy idiom and `apply_recv_to_world`'s signature — the consumer
    /// passes a fresh `world.proxy_mut()` each call. (The §2g sketch wrote
    /// `&mut W`; a proxy is not itself `&mut`-reusable, so by-value is the
    /// accurate form.)
    ///
    /// # Precondition
    /// Workers parked (or not spawned). The bevy adapter's `ReceivePackets` set
    /// brackets this with park/unpark (G8); the synchronous oracle needs none.
    pub fn receive<W: WorldMutType<E>>(&mut self, world: W) -> ReceiveOutput<E> {
        let (coord, mut recv, send) = self.take_handles();
        let server_tick = coord.current_tick();

        // D0: single synchronous recv-drain.
        let mut output = recv.receive();

        // D0 (cont.): apply decoded entity ops to `world`. Skip the reassembly
        // when nothing arrived (matches the bevy `drain_recv_impl_split` early
        // return) — the handles round-trip unchanged.
        let (coord, recv, send) = if output.is_empty() {
            (coord, recv, send)
        } else {
            crate::pipeline_actors::apply_recv_to_world(
                coord,
                recv,
                send,
                world,
                &mut output,
                server_tick,
            )
        };

        self.restore_handles(coord, recv, send);
        output
    }

    /// **D8 + D9 — send.** Send-prep (preamble → scope → refresh), core snapshot
    /// build, then prepare + transmit of the frozen send job against `world`.
    ///
    /// This is the synchronous (oracle) execution shape: it prepares the frozen
    /// `SendPlan` at the freeze point and transmits it **inline** on the calling
    /// thread (§2g item 5 "Pipelined-oracle"). The worker-driven production shape
    /// (publish the job to the send slot, worker transmits next tick) is added
    /// with the runtime in G7-3; both reduce to the same `(snapshot, plan)` so
    /// they are byte-identical (G9pre §2i, empirically incl. the real assembler —
    /// `g9pre_core_assembler_*`).
    ///
    /// Delegates the load-bearing ordering to [`Self::drain_and_send`] (the
    /// D0–D9 drain-phase contract). `world` is the consumer's **live** entity
    /// world; the trimmed `SnapshotWorld` is assembled from it internally — the
    /// consumer never authors a snapshot.
    pub fn send<W: WorldRefType<E> + Sync>(&mut self, world: &W) {
        let (coord, recv, mut send) = self.take_handles();
        Self::drain_and_send(&coord, &mut send, world);
        self.restore_handles(coord, recv, send);
    }

    /// The **D0–D9 drain-phase ordering contract** (MISSION_PIPELINE_API_BOUNDARY
    /// §2g, audit N1). This is the single, first-class total order in which
    /// queued mutations and the send job apply — the byte-exactness-critical
    /// sequence that §2e's "queue from any system" ergonomics relocated out of
    /// cyberlith's hand-commented `do_park_window_tick` and into naia. The moat
    /// DETECTS a regression; this contract PREVENTS one.
    ///
    /// D0 (recv events → world) is owned by [`Self::receive`]. D1–D7 are the
    /// queued-op drains whose queues are introduced by later steps (G4/G5 entity
    /// registrations, G6 resource carriers, G5b authority/editor, G6b host-sync,
    /// §2e outbound messages, scope-ledger writes); each is a no-op early-return
    /// until its queue exists, and any future op class MUST be slotted explicitly
    /// into this order rather than appended ad hoc. D8/D9 are live now.
    fn drain_and_send<W: WorldRefType<E> + Sync>(
        coord: &CoordHandle<E>,
        send: &mut SendHandle<E>,
        world: &W,
    ) {
        // D1 entity-replication registrations (spawn_replicated/enable_replication)
        //    — queue introduced by G4/G5. Must drain before resource & host-sync.
        // D2 resource-replication registrations (Res<R> carriers) — G6. Before host-sync.
        // D3 lifecycle (despawn/insert/remove) — G4/G5. After spawns, before authority.
        // D4 authority/editor ops (take_authority) — G5b. Before host-sync.
        // D5 host-sync (change-detection → repl config) — G6b. After all entity mutations.
        // D6 outbound messages (send_message/broadcast, incl. desync snapshot) — §2e.
        // D7 scope-ledger writes (ScopeToggled enqueue) — before send-prep.
        //    (D1–D7: no queues exist yet at G7; intentionally empty — see doc.)

        // D8 send-prep — STRICT sub-order (`server_access.rs:1691-1706`):
        //    ack-drain (consume the cross-half ACK channel: trim acked
        //               `sent_updates` + fire delivery notifications)
        //    → preamble (drain room/configure, flush handshake+heartbeats)
        //    → scope-changes (publish freshly-scoped entities into per-user sends)
        //    → refresh (recompute the cross-thread needed-set FROM those).
        // Reordering these trims the snapshot wrong.
        //
        // The ack-drain is NOT optional and NOT folded into the preamble:
        // `apply_pending_send_preamble` flushes/heartbeats/empty-acks but does
        // NOT consume the inbound ACK channel — that single-owner consumer is
        // `drain_all_acks`, which both reference send paths run before transmit
        // (resident `SendState::send_all_packets`, the active send worker's
        // preamble). Omitting it would leave acked `sent_updates` in the
        // `RetransmitLedger` forever → endless retransmits and a needed-set that
        // never clears → divergence from resident the instant a client acks
        // (audit #1).
        send.drain_all_acks();
        send.apply_pending_send_preamble();
        send.apply_pending_scope_changes(world);
        send.refresh_needed_entities();

        // D9 snapshot build + send job.
        //    The needed-set is now final (post-D8), so the core assembler reads
        //    exactly what this send transmits. Build BEFORE prepare_send_job,
        //    which freezes the DiffMasks and clears the live per-user masks.
        //
        // Machine-pin the D8→D9 ordering invariant the spec promised (audit #4):
        // the send-prep preamble + scope-changes MUST have run this tick before
        // the snapshot is assembled, else the needed-set is stale and the trim
        // is wrong. (refresh runs immediately after scope above, in literal
        // sequence, so the preamble+scope flags are the load-bearing guard.)
        debug_assert!(
            send.send_prep_done_this_tick(),
            "D9 snapshot build ran before D8 send-prep (preamble+scope) this tick",
        );
        let snapshot = coord.send_state_view().build_needed_snapshot(world);
        // prepare_send_job re-runs preamble/scope idempotently (per-tick flags
        // set by D8 above), then freezes the plan against the live `world`.
        let plan = send.prepare_send_job(world);
        // Oracle/synchronous transmit shape: inline against the frozen snapshot.
        send.transmit_send_job(snapshot, plan);
    }
}

// ── TickCtx ───────────────────────────────────────────────────────────────────

/// Scoped context passed to a [`PipelinedServer::tick`] body.
///
/// Provides framework-agnostic access to all three pipeline handles plus the
/// consumer's world. The recv and send handles are **owned** for the duration
/// of the tick body; [`PipelinedServer::tick`] automatically returns them to their
/// slots when the body returns.
///
/// Adapters (e.g., `naia-bevy-server`) may add extension methods by `impl`ing
/// `TickCtx<'_, Entity, SpecificWorldType>` in their own crate.
pub struct TickCtx<'a, E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>> {
    /// Coordination handle (borrowed from the pipeline for this tick).
    pub coord: &'a mut CoordHandle<E>,
    /// Receive handle (owned by this context for this tick).
    pub recv: RecvHandle<E>,
    /// Send handle (owned by this context for this tick).
    pub send: SendHandle<E>,
    /// Consumer's world (framework-specific).
    pub world: &'a mut W,
}
