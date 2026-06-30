//! [`PipelinedWorldServer<E>`] — unified pipeline handle; the primary entry point
//! for consumers of the pipelined server API.
//!
//! Construct with [`PipelinedWorldServer::new`] (replaces `spawn_server_handles`).
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
//! must be parked before calling `tick()`; [`PipelinedWorldServer`] does NOT own
//! the park/unpark machinery — that belongs to the adapter
//! (`PluginInternalState::park_workers`).
//!
//! # Worker slot sharing
//!
//! [`PipelinedWorldServer::recv_slot`] and [`PipelinedWorldServer::send_slot`] return
//! `Arc<Mutex<Option<...>>>` clones. The bevy adapter gives these to the
//! recv/send workers at spawn time; workers deposit their handle before
//! parking and re-claim it after unpark.

use std::{hash::Hash, net::SocketAddr, sync::Arc};

use crossbeam_channel::Receiver;
use parking_lot::Mutex;
use naia_shared::{EntityAuthStatus, Protocol, Tick, WorldMutType, WorldRefType};

use crate::{
    room::RoomKey, server::ServerShared, user::UserKey, EntityOwner, ReceiveOutput, RecvHandle,
    ReplicationConfig, SendHandle, ServerConfig, InternalWorldServer,
};

use super::handles::CoordHandle;
use super::ServerEntityConverter;
use super::{PipelineRuntime, RuntimeState, RuntimeTimingHooks};

// ── PipelinedWorldServer ───────────────────────────────────────────────────────────

/// Unified pipeline handle. Construct with [`PipelinedWorldServer::new`].
///
/// See module-level docs for the design overview.
pub struct PipelinedWorldServer<E: Copy + Eq + Hash + Send + Sync + 'static> {
    /// Coordination handle (main-thread only). Stored as `Option` so the
    /// handle can be temporarily moved into a `InternalWorldServer` for operations
    /// that require full-server access (e.g., `io_load` via [`Self::listen`]).
    pub(super) coord: Option<CoordHandle<E>>,
    /// Park-window slot shared with the recv worker.
    recv_slot: Arc<Mutex<Option<RecvHandle<E>>>>,
    /// Park-window slot shared with the send worker.
    send_slot: Arc<Mutex<Option<SendHandle<E>>>>,
    /// MISSION_PIPELINE_API_BOUNDARY G8 (§2l Decision 1) — the send-shape
    /// selector for [`Self::send`]:
    /// - `None` ⇒ **oracle**: `send` transmits the frozen job INLINE on the
    ///   calling thread (the determinism/desync path the moat validates, and
    ///   every synchronous test drive). Main drains the ACK channel itself.
    /// - `Some(tx)` ⇒ **worker-driven production**: `send` publishes the frozen
    ///   `(snapshot, plan)` job here and the send worker transmits it NEXT tick
    ///   (one-tick lag, MISSION_TICK_FLOOR Lever 3). Main does NOT drain ACKs —
    ///   the send worker is the single-owner consumer (`runtime.rs`).
    ///
    /// Set by the bevy adapter under `#[cfg(workers_active)]` via
    /// [`Self::set_send_publisher`]; left `None` in deterministic builds. Both
    /// shapes reduce to the same `(snapshot, frozen plan)` and so emit
    /// byte-identical wire content modulo the one-tick scheduling shift
    /// (G9pre §2i).
    send_publisher: Option<super::SnapshotSender<E>>,
    /// MISSION_PIPELINE_API_BOUNDARY G8b — the recv-shape selector for
    /// [`Self::receive`], the exact mirror of [`Self::send_publisher`]:
    /// - `None` ⇒ **oracle**: `receive` drains ONLY a synchronous
    ///   `recv.receive()` on the calling thread (the determinism/desync path and
    ///   every synchronous test drive). There is no recv worker.
    /// - `Some(rx)` ⇒ **worker-driven production**: `receive` first drains
    ///   everything the recv WORKER has shipped to its output channel since the
    ///   last park window (FIFO, via `rx.try_iter()`), THEN appends one
    ///   synchronous `recv.receive()` straggler-catch — exactly the sequence the
    ///   bevy adapter previously hand-rolled in `drain_recv_impl`.
    ///
    /// Set by the bevy adapter under `#[cfg(not(feature = "deterministic"))]` via
    /// [`Self::set_recv_subscriber`] (wired to the SAME channel whose `Sender` the
    /// [`super::PipelineRuntime`]'s recv worker pushes to); left `None` in
    /// deterministic builds. Both shapes feed the same `apply_recv_to_world`
    /// application path, so the only difference is WHERE the socket drain happens
    /// (this thread vs the recv worker) — symmetric to the send-shape split.
    recv_subscriber: Option<Receiver<ReceiveOutput<E>>>,
    /// MISSION_PIPELINE_API_BOUNDARY §2f — the worker-thread runtime, OWNED by
    /// the pipelined server (was the bevy adapter's `PluginInternalState.runtime`
    /// until the §2f ownership move). `None` until [`Self::start_workers`] spawns
    /// the threads (the separate spawn step after [`Self::listen`] binds the
    /// socket); always `None` in deterministic builds (`not(workers_active)` — the
    /// synchronous oracle shape, no threads). When `Some` + `Running`,
    /// [`Self::receive`] parks the workers at the top of the window and
    /// [`Self::send`] unparks them at the bottom, so the whole park-window bracket
    /// is self-contained in naia core. The runtime's `Drop` joins the workers.
    runtime: Option<PipelineRuntime<E>>,
}

impl<E: Copy + Eq + Hash + Send + Sync + 'static> PipelinedWorldServer<E> {
    /// Construct a [`InternalWorldServer<E>`] and immediately split it into a
    /// [`PipelinedWorldServer<E>`].
    ///
    /// This is the primary entry point for consumers. Pass the result to the
    /// bevy adapter's `Plugin::pipelined` (via a `PipelinedWorldServer` Bevy
    /// resource), then call [`Self::listen`] to bind a socket and drive ticks
    /// with [`Self::tick`].
    pub fn new<P: Into<Protocol>>(server_config: ServerConfig, protocol: P) -> Self {
        let ws = InternalWorldServer::<E>::new(server_config, protocol);
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
            send_publisher: None,
            recv_subscriber: None,
            runtime: None,
        }
    }

    /// MISSION_PIPELINE_API_BOUNDARY G8 (§2l Decision 1) — switch [`Self::send`]
    /// into **worker-driven production** mode: it will publish the frozen
    /// `(snapshot, plan)` send job to `publisher` (drained by the send worker,
    /// which transmits it next tick — the one-tick lag) instead of transmitting
    /// inline.
    ///
    /// The bevy adapter calls this under `#[cfg(workers_active)]` with a clone of
    /// the same [`super::SnapshotSender`] whose paired [`super::SnapshotReceiver`]
    /// was handed to the [`super::PipelineRuntime`]'s send worker. In
    /// deterministic builds it is never called, so `send` stays the inline oracle.
    ///
    /// Call before workers are spawned (the publisher must be wired before any
    /// `send`).
    pub fn set_send_publisher(&mut self, publisher: super::SnapshotSender<E>) {
        self.send_publisher = Some(publisher);
    }

    /// MISSION_PIPELINE_API_BOUNDARY G8b (mirror of [`Self::set_send_publisher`])
    /// — switch [`Self::receive`] into **worker-driven production** mode: it will
    /// drain `subscriber` (the recv worker's output channel) before its
    /// synchronous straggler-catch, instead of only doing the synchronous drain.
    ///
    /// The bevy adapter calls this under `#[cfg(not(feature = "deterministic"))]`
    /// with the [`Receiver`] returned by the [`super::PipelineRuntime`]'s
    /// `recv_out_receiver()` (the consumer side of the channel the recv worker
    /// pushes to). In deterministic builds it is never called, so `receive` stays
    /// the inline oracle. Call before workers are spawned.
    pub fn set_recv_subscriber(&mut self, subscriber: Receiver<ReceiveOutput<E>>) {
        self.recv_subscriber = Some(subscriber);
    }

    /// Borrow the coordination handle.
    ///
    /// Panics if `coord` is temporarily absent (only during [`Self::tick`] or
    /// [`Self::with_world_server`] — both restore it before returning).
    pub fn coord(&self) -> &CoordHandle<E> {
        self.coord.as_ref().expect("PipelinedWorldServer: CoordHandle temporarily unavailable")
    }

    /// Mutably borrow the coordination handle.
    pub fn coord_mut(&mut self) -> &mut CoordHandle<E> {
        self.coord.as_mut().expect("PipelinedWorldServer: CoordHandle temporarily unavailable")
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
            "PipelinedWorldServer::tick: CoordHandle not available — re-entrant tick?",
        );
        let recv = self.recv_slot.lock().take().expect(
            "PipelinedWorldServer::tick: RecvHandle not in slot — park workers before calling tick()",
        );
        let send = self.send_slot.lock().take().expect(
            "PipelinedWorldServer::tick: SendHandle not in slot — park workers before calling tick()",
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
        self.coord.take().expect("PipelinedWorldServer::take_coord: CoordHandle not available")
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
            "PipelinedWorldServer::take_handles: CoordHandle not available",
        );
        let recv = self.recv_slot.lock().take().expect(
            "PipelinedWorldServer::take_handles: RecvHandle not in slot — park workers first",
        );
        let send = self.send_slot.lock().take().expect(
            "PipelinedWorldServer::take_handles: SendHandle not in slot — park workers first",
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
    /// calls `io_load` on the reassembled `InternalWorldServer`.
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

    /// §2f — the **separate spawn step**: stand up the worker-thread runtime and
    /// transition it `Armed → Running`. Call AFTER [`Self::listen`] has bound the
    /// socket (so the recv handle's `readiness()` is meaningful) and while the
    /// handles are in their slots.
    ///
    /// Creates the internal snapshot lag channel (so `send` publishes the frozen
    /// `(snapshot, plan)` job to the send worker — the consumer never authors a
    /// snapshot, §2f), wires the recv worker's output channel into
    /// [`Self::recv_subscriber`], and spawns the threads around this pipeline's own
    /// slot `Arc`s. `timing` carries the optional per-stage instrumentation hooks
    /// (zero-overhead `None`s for non-bench builds).
    ///
    /// In deterministic builds (`not(workers_active)`) this is a no-op: there are
    /// no worker threads and `receive`/`send` run the synchronous oracle shape.
    #[cfg(workers_active)]
    pub fn start_workers(&mut self, timing: RuntimeTimingHooks) {
        // The snapshot job channel is naia-internal: `send` publishes here, the
        // send worker transmits next tick (the one-tick lag).
        let (snap_sender, snap_receiver) = super::SnapshotSender::<E>::pair();
        self.send_publisher = Some(snap_sender);

        let runtime = PipelineRuntime::new_armed(
            Arc::clone(&self.recv_slot),
            Arc::clone(&self.send_slot),
            snap_receiver,
            timing,
        );
        // Mirror of the send wiring: point `receive` at the recv worker's output
        // channel (born inside `new_armed`).
        if let Some(rx) = runtime.recv_out_receiver() {
            self.recv_subscriber = Some(rx);
        }

        // The recv handle is back in its slot after `listen()`; read its readiness
        // so the recv worker can select on it instead of polling.
        let recv_readiness = self
            .recv_slot
            .lock()
            .as_ref()
            .expect("recv handle in slot before start_workers()")
            .readiness();

        runtime.spawn_workers(recv_readiness);
        self.runtime = Some(runtime);
    }

    /// Deterministic-build no-op form of [`Self::start_workers`] (no worker
    /// threads exist; `receive`/`send` run the synchronous oracle shape).
    #[cfg(not(workers_active))]
    pub fn start_workers(&mut self, _timing: RuntimeTimingHooks) {}

    /// `true` once [`Self::start_workers`] has spawned the runtime and it is
    /// `Running` (so `receive`/`send` should park/unpark around the window).
    /// Always `false` in deterministic builds.
    pub fn is_running(&self) -> bool {
        self.runtime
            .as_ref()
            .map_or(false, |rt| rt.state() == RuntimeState::Running)
    }

    /// If an owned worker thread has panicked, re-panic on the calling thread.
    /// No-op when no runtime is present (deterministic / not-yet-spawned).
    pub fn propagate_panic_if_any(&self) {
        if let Some(rt) = &self.runtime {
            rt.propagate_panic_if_any();
        }
    }

    /// Test/dev hook: request that the owned workers panic on their next loop
    /// iteration. No-op when no runtime is present.
    #[cfg(any(test, feature = "test_time"))]
    pub fn request_worker_panic_for_test(&self) {
        if let Some(rt) = &self.runtime {
            rt.request_worker_panic_for_test();
        }
    }

    /// Temporarily reassemble a [`crate::InternalWorldServer`] and invoke `f` against it.
    ///
    /// All three handles are moved into the `InternalWorldServer`; after `f` returns
    /// they are re-split and restored. Used for ops that require full-server
    /// access (e.g. `entity_replication_config` until G3 adds it to `CoordHandle`).
    ///
    /// Workers must be parked (or not yet started) before calling this.
    pub fn with_world_server<R>(
        &mut self,
        f: impl FnOnce(&mut crate::InternalWorldServer<E>) -> R,
    ) -> R {
        let coord = self.coord.take().expect(
            "PipelinedWorldServer::with_world_server: CoordHandle not available",
        );
        let recv = self.recv_slot.lock().take().expect(
            "PipelinedWorldServer::with_world_server: RecvHandle not in slot",
        );
        let send = self.send_slot.lock().take().expect(
            "PipelinedWorldServer::with_world_server: SendHandle not in slot",
        );

        let coord_state = coord.state;
        let mut ws = InternalWorldServer::from_pipeline_states(coord_state, recv.state, send.state);
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
// method so consumers operate on the unified [`PipelinedWorldServer<E>`] handle
// directly — no `pipeline.coord_mut().method()` ceremony, no `InternalWorldServer`
// reassembly. All methods delegate to `self.coord()` / `self.coord_mut()`,
// which panic only inside an in-flight `tick`/`with_world_server` window (the
// pipelined consumer calls these from the parked main thread, outside that
// window). `enable_entity_replication` is intentionally NOT forwarded here —
// it is superseded by the G5 `enable_replication_for_existing_entity` op.

impl<E: Copy + Eq + Hash + Send + Sync + 'static> PipelinedWorldServer<E> {
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

    /// Forwards to [`CoordHandle::entity_replication_config`].
    pub fn entity_replication_config(
        &self,
        world_entity: &E,
    ) -> Option<crate::ReplicationConfig> {
        self.coord().entity_replication_config(world_entity)
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

    /// Forwards to [`CoordHandle::enable_entity_replication`].
    pub fn enable_entity_replication(&mut self, world_entity: &E) {
        self.coord_mut().enable_entity_replication(world_entity)
    }

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

    /// Forwards to [`CoordHandle::historian`].
    pub fn historian(&self) -> Option<&crate::historian::Historian> {
        self.coord().historian()
    }

    /// Forwards to [`CoordHandle::average_tick_duration`].
    pub fn average_tick_duration(&self) -> std::time::Duration {
        self.coord().average_tick_duration()
    }

    /// Forwards to [`CoordHandle::entity_count`].
    pub fn entity_count(&self) -> usize {
        self.coord().entity_count()
    }

    /// Forwards to [`CoordHandle::users_count`].
    pub fn users_count(&self) -> usize {
        self.coord().users_count()
    }

    /// Forwards to [`CoordHandle::user_count`].
    pub fn user_count(&self) -> usize {
        self.coord().user_count()
    }

    /// Forwards to [`CoordHandle::room_exists`].
    pub fn room_exists(&self, room_key: &RoomKey) -> bool {
        self.coord().room_exists(room_key)
    }

    /// Forwards to [`CoordHandle::room_keys`].
    pub fn room_keys(&self) -> Vec<RoomKey> {
        self.coord().room_keys()
    }

    /// Forwards to [`CoordHandle::rooms_count`].
    pub fn rooms_count(&self) -> usize {
        self.coord().rooms_count()
    }

    /// Forwards to [`CoordHandle::room_count`].
    pub fn room_count(&self) -> usize {
        self.coord().room_count()
    }

    /// Forwards to [`CoordHandle::has_resource`].
    pub fn has_resource<R: naia_shared::ReplicatedComponent>(&self) -> bool {
        self.coord().has_resource::<R>()
    }

    /// Forwards to [`CoordHandle::resource_entity`].
    pub fn resource_entity<R: naia_shared::ReplicatedComponent>(&self) -> Option<E> {
        self.coord().resource_entity::<R>()
    }

    /// Forwards to [`CoordHandle::resources_count`].
    pub fn resources_count(&self) -> usize {
        self.coord().resources_count()
    }

    /// Forwards to [`CoordHandle::resource_authority_status`].
    pub fn resource_authority_status<R: naia_shared::ReplicatedComponent>(
        &self,
    ) -> Option<EntityAuthStatus> {
        self.coord().resource_authority_status::<R>()
    }

    /// Forwards to [`CoordHandle::global_entity_to_entity`].
    pub fn global_entity_to_entity(
        &self,
        global_entity: &naia_shared::GlobalEntity,
    ) -> Result<E, naia_shared::EntityDoesNotExistError> {
        self.coord().global_entity_to_entity(global_entity)
    }

    /// Forwards to [`CoordHandle::entity_to_global_entity`].
    pub fn entity_to_global_entity(
        &self,
        world_entity: &E,
    ) -> Result<naia_shared::GlobalEntity, naia_shared::EntityDoesNotExistError> {
        self.coord().entity_to_global_entity(world_entity)
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

impl<E: Copy + Eq + Hash + Send + Sync + 'static> PipelinedWorldServer<E> {
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
    /// **Send-symmetric dual recv-shape (G8b).** `world` is taken as `&mut W`
    /// (`W: WorldMutType<E>`) so the several `ReceiveOutput`s a worker-shape
    /// channel burst can yield in one tick are each applied against a reborrow of
    /// the same world (a `WorldMutType` proxy is single-use by value). The oracle
    /// shape yields exactly one output; both return a `Vec` for a uniform API.
    ///
    /// Returns the per-output [`ReceiveOutput`]s — the connection-scoped and
    /// entity-scoped EVENTS — in FIFO order, with each output's `world_events`
    /// already populated by its application, for the consumer (or its framework
    /// adapter) to fan out. Core mutates only `world` (the single entity world,
    /// §2h H3); it routes no events itself.
    ///
    /// Shape selected by [`Self::recv_subscriber`]:
    /// - `None` (oracle): one synchronous `recv.receive()`.
    /// - `Some` (worker production): drain the recv worker's output channel FIFO,
    ///   then append one synchronous `recv.receive()` straggler-catch. This is the
    ///   exact sequence the bevy adapter's `drain_recv_impl` hand-rolled; G8b
    ///   folds it into core so `receive` is the single recv entry point.
    ///
    /// # Precondition
    /// Workers parked (or not spawned). The bevy adapter's `ReceivePackets` set
    /// brackets this with park/unpark (G8); the synchronous oracle needs none.
    pub fn receive<W: WorldMutType<E>>(&mut self, world: &mut W) -> Vec<ReceiveOutput<E>> {
        // §2f: when the worker runtime is owned + `Running`, OPEN the park window
        // here — the workers deposit their handles in the slots and block, so the
        // `take_handles` below is race-free. No-op for the synchronous oracle
        // (no runtime) — byte-identical to the pre-ownership-move bracket.
        if self.is_running() {
            self.runtime.as_ref().unwrap().park_workers();
        }
        let (mut coord, mut recv, mut send) = self.take_handles();
        let server_tick = coord.current_tick();

        // Worker shape: drain everything the recv worker shipped to its output
        // channel since the last park window, FIFO (unbounded → nothing dropped).
        // Oracle shape (`recv_subscriber == None`): there is no worker channel.
        let mut outputs: Vec<ReceiveOutput<E>> = match &self.recv_subscriber {
            Some(rx) => rx.try_iter().collect(),
            None => Vec::new(),
        };
        // Synchronous straggler-catch (BOTH shapes): in worker mode this catches
        // packets delivered after the recv worker's last iteration; in oracle mode
        // it is the sole drain.
        outputs.push(recv.receive());

        // D0 (cont.): apply each non-empty output's decoded entity ops to `world`
        // in FIFO order, threading the handles. Reborrow `world` per output. Empty
        // outputs skip the `InternalWorldServer` reassembly (matches the prior early
        // return) — the handles round-trip unchanged.
        for output in outputs.iter_mut() {
            if output.is_empty() {
                continue;
            }
            let (c, r, s) = crate::pipeline_actors::apply_recv_to_world(
                coord,
                recv,
                send,
                &mut *world,
                output,
                server_tick,
            );
            coord = c;
            recv = r;
            send = s;
        }

        self.restore_handles(coord, recv, send);
        outputs
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
        // Take the publisher out for the duration of the send so `drain_and_send`
        // can borrow it without aliasing `&mut self` (the handles are also taken
        // from `self`). It is a plain clone-able channel sender; move it back after.
        let publisher = self.send_publisher.take();
        let (coord, recv, mut send) = self.take_handles();
        Self::drain_and_send(&coord, &mut send, world, publisher.as_ref());
        self.restore_handles(coord, recv, send);
        self.send_publisher = publisher;
        // §2f: CLOSE the park window — resume the workers (the send worker picks
        // up the freshly-published frozen job and transmits it next tick). No-op
        // for the synchronous oracle (no runtime).
        if self.is_running() {
            self.runtime.as_ref().unwrap().unpark_workers();
        }
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
        publisher: Option<&super::SnapshotSender<E>>,
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
        //
        // **Send-shape split (G8 §2l Decision 1):** the ack-drain owner depends
        // on the shape. In the ORACLE shape (`publisher = None`) the calling
        // thread is the sole owner of the send half, so it drains here. In the
        // WORKER shape (`publisher = Some`) the send WORKER drains the channel in
        // its own preamble before each transmit (`runtime.rs`) — it is the
        // single-owner consumer, so main MUST NOT drain too (double-consume would
        // trim `sent_updates` a tick early → divergence). preamble/scope/refresh
        // run in BOTH shapes (they prepare this tick's per-user sends + needed-set
        // / frozen plan, which the worker then transmits self-contained).
        if publisher.is_none() {
            send.drain_all_acks();
        }
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
        let mut snapshot = coord.send_state_view().build_needed_snapshot(world);
        // prepare_send_job re-runs preamble/scope idempotently (per-tick flags
        // set by D8 above), then freezes the plan against the live `world`.
        let plan = send.prepare_send_job(world);
        match publisher {
            // ORACLE shape: inline transmit against the frozen snapshot, on the
            // calling thread (no workers / the synchronous determinism path).
            None => send.transmit_send_job(snapshot, plan),
            // WORKER-DRIVEN PRODUCTION shape (G8 §2l Decision 1): attach the
            // frozen plan to the snapshot, making it a self-contained send job,
            // and publish it. The send worker drains the latest job, drains the
            // ACK channel, and transmits NEXT tick (one-tick lag). This preserves
            // the worker overlap the perf floor depends on; the worker's transmit
            // reads zero live per-user diff state because the plan is frozen here.
            Some(publisher) => {
                snapshot.attach_send_plan(plan);
                publisher.send(snapshot);
            }
        }
    }
}

// ── TickCtx ───────────────────────────────────────────────────────────────────

/// Scoped context passed to a [`PipelinedWorldServer::tick`] body.
///
/// Provides framework-agnostic access to all three pipeline handles plus the
/// consumer's world. The recv and send handles are **owned** for the duration
/// of the tick body; [`PipelinedWorldServer::tick`] automatically returns them to their
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
