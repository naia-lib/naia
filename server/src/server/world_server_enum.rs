//! The unified, framework-agnostic server handle (MISSION_PIPELINE_API_BOUNDARY
//! G-unify Phase 2c).
//!
//! [`WorldServer<E>`] is the ONE public face over the two engine shapes:
//!
//! - [`WorldServerImpl::Resident`] — the fused [`InternalWorldServer`] driven
//!   synchronously on the calling thread.
//! - [`WorldServerImpl::Pipelined`] — the *same* engine's three handles split
//!   across worker threads ([`PipelinedWorldServer`]), assembled into a
//!   transient [`InternalWorldServer`] view per drive.
//!
//! Both variants are built from the same three handles (`CoordHandle` /
//! `RecvHandle` / `SendHandle`), so a consumer holds ONE type, picks its drive
//! shape at construction ([`WorldServer::new`] vs [`WorldServer::new_pipelined`]),
//! and gets the same op surface + `receive`/`send` drives dispatched per variant.
//! This is what makes pipelining ergonomic for non-bevy naia consumers without
//! reassembling the engine by hand.
//!
//! The op/drive surface is grown across the G-unify phases: Phase 2c stands up
//! the shell (construction + lifecycle); Phase 3 adds the unified `receive`/`send`
//! drives; Phase 4 adds the `entity_mut` builder.

use std::{hash::Hash, net::SocketAddr, time::Duration};

use naia_shared::{
    AuthorityError, Channel, ComponentKind, ConnectionStats, DisconnectReason,
    EntityAndGlobalEntityConverter, EntityAuthStatus, EntityDoesNotExistError, GlobalEntity,
    Instant, Message, Protocol, Replicate, ReplicatedComponent, Request, ResourceAlreadyExists,
    Response, ResponseReceiveKey, ResponseSendKey, SendPlan, Tick, WorldMutType, WorldRefType,
};

use crate::{
    events::{world_events::WorldEvents, TickEvents},
    pipeline_actors::SendStateView,
    world::entity_mut::{EntityMut, EntityMutTarget},
    world::entity_ref::{EntityRef, EntityRefTarget},
    EntityOwner, EntityPriorityMut, EntityPriorityRef, Historian, InternalWorldServer,
    NaiaServerError, PipelinedWorldServer, ReceiveOutput, ReplicationConfig, ResponseSendOutcome,
    RoomKey, RoomMut,
    RoomRef, ServerConfig, TickBufferMessages, UserKey, UserMut, UserRef, UserScopeMut,
    UserScopeRef,
};

/// Whether a [`WorldServer`] drives its engine synchronously (resident) or
/// across the pipeline's worker threads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerMode {
    /// Synchronous, single-threaded drive over the fused engine.
    Resident,
    /// Worker-thread pipelined drive (park-window brackets).
    Pipelined,
}

/// The unified server handle. See the module docs.
pub struct WorldServer<E: Copy + Eq + Hash + Send + Sync + 'static> {
    inner: WorldServerImpl<E>,
}

/// Internal variant carrier. Kept private so the only way to act on a
/// [`WorldServer`] is through its dispatched surface (no variant-pattern leak).
enum WorldServerImpl<E: Copy + Eq + Hash + Send + Sync + 'static> {
    Resident(InternalWorldServer<E>),
    Pipelined(PipelinedWorldServer<E>),
}

impl<E: Copy + Eq + Hash + Send + Sync + 'static> WorldServer<E> {
    /// Construct a **resident** server: the fused engine, driven synchronously.
    pub fn new<P: Into<Protocol>>(server_config: ServerConfig, protocol: P) -> Self {
        Self {
            inner: WorldServerImpl::Resident(InternalWorldServer::new(server_config, protocol)),
        }
    }

    /// Construct a **pipelined** server: the engine's handles split across the
    /// worker-thread park-window runtime.
    pub fn new_pipelined<P: Into<Protocol>>(server_config: ServerConfig, protocol: P) -> Self {
        Self {
            inner: WorldServerImpl::Pipelined(PipelinedWorldServer::new(server_config, protocol)),
        }
    }

    /// Wrap an already-constructed [`PipelinedWorldServer`] (e.g. one built via
    /// `spawn_server_handles` with extra wiring) as a pipelined [`WorldServer`].
    pub fn from_pipelined(pipeline: PipelinedWorldServer<E>) -> Self {
        Self {
            inner: WorldServerImpl::Pipelined(pipeline),
        }
    }

    /// Add `world_entity` to `room_key` (coarse scope membership). First-class
    /// over both modes (the unified replacement for the former raw coord-handle
    /// `room_add_entity`).
    pub fn room_add_entity(&mut self, room_key: &RoomKey, world_entity: &E) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.room_add_entity(room_key, world_entity),
            WorldServerImpl::Pipelined(ps) => ps.room_add_entity(room_key, world_entity),
        }
    }

    /// Remove `world_entity` from `room_key`.
    pub fn room_remove_entity(&mut self, room_key: &RoomKey, world_entity: &E) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.room_remove_entity(room_key, world_entity),
            WorldServerImpl::Pipelined(ps) => ps.room_remove_entity(room_key, world_entity),
        }
    }

    /// Disconnect a user out-of-band (coord-inline). First-class over both modes.
    pub fn disconnect_user(&mut self, user_key: &UserKey) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.sim_handle.disconnect_user(user_key),
            WorldServerImpl::Pipelined(ps) => ps.disconnect_user(user_key),
        }
    }

    /// Flush the batched world-mutation hooks staged during this tick's
    /// coord-side ops (entity registration / configuration) onto `world`. In
    /// resident mode the staging queue is normally empty (ops apply inline), so
    /// this is a cheap drain; in pipelined mode it applies the deferred hooks.
    pub fn apply_pending_world_hooks<W: WorldMutType<E>>(&self, world: &mut W) {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.sim_handle.apply_pending_world_hooks(world),
            WorldServerImpl::Pipelined(ps) => ps.apply_pending_world_hooks(world),
        }
    }

    /// Take the three engine handles out for a worldless drain (the bevy
    /// host-sync phase). Pipelined-only — restore with [`Self::restore_handles`].
    ///
    /// # Panics
    /// Panics on a resident server (the handles live inline, not in slots — there
    /// is nothing to take). Calling this on a resident server is a misuse.
    pub fn take_handles(
        &mut self,
    ) -> (
        crate::CoordHandle<E>,
        crate::RecvHandle<E>,
        crate::SendHandle<E>,
    ) {
        match &mut self.inner {
            WorldServerImpl::Pipelined(ps) => ps.take_handles(),
            WorldServerImpl::Resident(_) => {
                panic!("WorldServer::take_handles called on a resident server (pipelined-only)")
            }
        }
    }

    /// Restore handles taken by [`Self::take_handles`]. Pipelined-only.
    ///
    /// # Panics
    /// Panics on a resident server.
    pub fn restore_handles(
        &mut self,
        coord: crate::CoordHandle<E>,
        recv: crate::RecvHandle<E>,
        send: crate::SendHandle<E>,
    ) {
        match &mut self.inner {
            WorldServerImpl::Pipelined(ps) => ps.restore_handles(coord, recv, send),
            WorldServerImpl::Resident(_) => {
                panic!("WorldServer::restore_handles called on a resident server (pipelined-only)")
            }
        }
    }

    /// Test/dev hook: ask the pipeline workers to panic on their next loop.
    /// Pipelined-only; no-op-shaped panic on a resident server.
    #[cfg(any(test, feature = "test_time"))]
    pub fn request_worker_panic_for_test(&self) {
        match &self.inner {
            WorldServerImpl::Pipelined(ps) => ps.request_worker_panic_for_test(),
            WorldServerImpl::Resident(_) => panic!(
                "WorldServer::request_worker_panic_for_test called on a resident server (pipelined-only)"
            ),
        }
    }

    /// §2f separate spawn step: stand up the pipelined worker runtime (after
    /// [`Self::listen`] binds the socket). Under `not(workers_active)` the
    /// pipelined arm is itself a no-op (the synchronous oracle has no threads).
    ///
    /// # Panics
    /// Panics on a resident server — spawning workers is a pipelined-only
    /// lifecycle action; calling it here is a misuse (fail loud, never silently
    /// no-op).
    pub fn start_workers(&mut self, timing: crate::pipeline_actors::RuntimeTimingHooks) {
        match &mut self.inner {
            WorldServerImpl::Pipelined(ps) => ps.start_workers(timing),
            WorldServerImpl::Resident(_) => {
                panic!("WorldServer::start_workers called on a resident server (pipelined-only)")
            }
        }
    }

    /// `true` if this is a pipelined server whose workers are spawned + running.
    /// Always `false` for a resident server.
    pub fn is_running(&self) -> bool {
        match &self.inner {
            WorldServerImpl::Pipelined(ps) => ps.is_running(),
            WorldServerImpl::Resident(_) => false,
        }
    }

    /// Re-panic on the calling thread if an owned pipelined worker has panicked.
    ///
    /// # Panics
    /// Panics on a resident server (no worker threads exist; calling this here
    /// is a misuse — fail loud).
    pub fn propagate_panic_if_any(&self) {
        match &self.inner {
            WorldServerImpl::Pipelined(ps) => ps.propagate_panic_if_any(),
            WorldServerImpl::Resident(_) => {
                panic!("WorldServer::propagate_panic_if_any called on a resident server (pipelined-only)")
            }
        }
    }

    /// Explicitly open the pipelined park window (advanced/test escape hatch;
    /// `receive` does this internally).
    ///
    /// # Panics
    /// Panics on a resident server (no park window exists — fail loud).
    pub fn park_workers(&self) {
        match &self.inner {
            WorldServerImpl::Pipelined(ps) => ps.park_workers(),
            WorldServerImpl::Resident(_) => {
                panic!("WorldServer::park_workers called on a resident server (pipelined-only)")
            }
        }
    }

    /// Like [`Self::park_workers`], but flushes the send pipeline to io first so
    /// snapshot delivery is deterministic under active workers (test/harness
    /// liveness barrier).
    ///
    /// # Panics
    /// Panics on a resident server (no park window exists — fail loud).
    pub fn park_workers_flushing(&self) {
        match &self.inner {
            WorldServerImpl::Pipelined(ps) => ps.park_workers_flushing(),
            WorldServerImpl::Resident(_) => {
                panic!(
                    "WorldServer::park_workers_flushing called on a resident server (pipelined-only)"
                )
            }
        }
    }

    /// Explicitly close the pipelined park window.
    ///
    /// # Panics
    /// Panics on a resident server (no park window exists — fail loud).
    pub fn unpark_workers(&self) {
        match &self.inner {
            WorldServerImpl::Pipelined(ps) => ps.unpark_workers(),
            WorldServerImpl::Resident(_) => {
                panic!("WorldServer::unpark_workers called on a resident server (pipelined-only)")
            }
        }
    }

    /// Which drive shape this server was constructed with.
    pub fn mode(&self) -> ServerMode {
        match &self.inner {
            WorldServerImpl::Resident(_) => ServerMode::Resident,
            WorldServerImpl::Pipelined(_) => ServerMode::Pipelined,
        }
    }

    /// Bind the transport socket and begin listening for clients. For the
    /// pipelined variant this reassembles the engine to run `io_load` exactly
    /// as the resident variant does (the worker threads are not yet spawned).
    pub fn listen<S: Into<Box<dyn crate::transport::Socket>>>(&mut self, socket: S) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => {
                let (_auth_tx, _auth_rx, ps, pr) = crate::transport::Socket::listen(socket.into());
                ws.io_load(ps, pr);
            }
            WorldServerImpl::Pipelined(ps) => ps.listen(socket),
        }
    }

    /// Load the world-io transport endpoints (sender + receiver). This is the
    /// channel-fed entry point used by [`crate::Server::listen`] (the io source is
    /// a [`crate::transport::PacketChannel`] driven by `MainServer`, not a socket).
    /// The pipelined arm feeds the same endpoints into the parked engine.
    pub fn io_load(
        &mut self,
        sender: Box<dyn crate::transport::PacketSender>,
        receiver: Box<dyn crate::transport::PacketReceiver>,
    ) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.io_load(sender, receiver),
            WorldServerImpl::Pipelined(ps) => {
                ps.with_monolithic_world_server(|ws| ws.io_load(sender, receiver))
            }
        }
    }

    /// The current server tick.
    pub fn current_tick(&self) -> Tick {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.current_tick(),
            WorldServerImpl::Pipelined(ps) => ps.current_tick(),
        }
    }

    /// Receive one tick's worth of inbound traffic and apply it to `world`,
    /// returning the per-source [`ReceiveOutput`]s (world + tick events).
    ///
    /// The world is taken **by value** — the established naia convention is a
    /// fresh per-call proxy (`world.proxy_mut()`). The resident variant drives
    /// its fused `receive_with_world` (one output); the pipelined variant drains
    /// its recv path (one output in the oracle shape, N in the worker shape).
    /// Both ultimately apply through the same `process_all_packets` path.
    pub fn receive<W: WorldMutType<E>>(&mut self, world: W) -> Vec<ReceiveOutput<E>> {
        let mut out = Vec::new();
        self.receive_into(world, &mut out);
        out
    }

    /// Buffer-reusing form of [`Self::receive`]: fills `out` (cleared first)
    /// with this tick's outputs so the caller can retain the allocation across
    /// ticks. The hot per-tick recv system uses this to avoid a per-tick
    /// `Vec` allocation; byte-identical to `receive`.
    pub fn receive_into<W: WorldMutType<E>>(
        &mut self,
        mut world: W,
        out: &mut Vec<ReceiveOutput<E>>,
    ) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => {
                out.clear();
                out.push(ws.receive_with_world(world));
            }
            WorldServerImpl::Pipelined(ps) => ps.receive_into(&mut world, out),
        }
    }

    /// Flush one tick's worth of outbound traffic, serialized against `world`.
    ///
    /// World by value (fresh per-call proxy, e.g. `world.proxy()`). The resident
    /// variant transmits inline against the live world; the pipelined variant
    /// transmits the frozen needed-set snapshot (oracle inline, or published to
    /// the send worker). Byte-identical modulo the worker's one-tick lag
    /// (g9pre).
    pub fn send<W: WorldRefType<E> + Sync>(&mut self, world: W) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.send_all_packets(world),
            WorldServerImpl::Pipelined(ps) => ps.send(&world),
        }
    }

    /// The imperative entity builder, dispatched per variant. Holds the `world`
    /// for component access. `entity` must already exist in `world`.
    ///
    /// ```ignore
    /// server.entity_mut(world, &entity)
    ///       .enable_replication()
    ///       .configure_replication(ReplicationConfig::public())
    ///       .enter_room(&room_key);
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if `entity` does not exist in `world` (matching the resident
    /// `InternalWorldServer::entity_mut` contract).
    pub fn entity_mut<W: WorldMutType<E>>(&mut self, world: W, entity: &E) -> EntityMut<'_, E, W> {
        if !world.has_entity(entity) {
            panic!("No Entity exists for given Key!");
        }
        let target = match &mut self.inner {
            WorldServerImpl::Resident(ws) => EntityMutTarget::Resident(ws),
            WorldServerImpl::Pipelined(ps) => EntityMutTarget::Pipelined(ps),
        };
        EntityMut::with_target(target, world, entity)
    }

    // ── Reads / owned returns ────────────────────────────────────────────

    /// Whether the server has bound its transport and is listening.
    pub fn is_listening(&self) -> bool {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.is_listening(),
            WorldServerImpl::Pipelined(ps) => ps.is_listening(),
        }
    }

    /// The current average tick duration of the server.
    pub fn average_tick_duration(&self) -> Duration {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.average_tick_duration(),
            WorldServerImpl::Pipelined(ps) => ps.average_tick_duration(),
        }
    }

    /// Server-side authority status for `entity`, or `None` if it is not
    /// configured as `Delegated`.
    pub fn entity_authority_status(&self, entity: &E) -> Option<EntityAuthStatus> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.entity_authority_status(entity),
            WorldServerImpl::Pipelined(ps) => ps.entity_authority_status(entity),
        }
    }

    /// The owner of `entity`.
    pub fn entity_owner(&self, entity: &E) -> EntityOwner {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.entity_owner(entity),
            WorldServerImpl::Pipelined(ps) => ps.entity_owner(entity),
        }
    }

    /// True iff a resource of type `R` is currently inserted.
    pub fn has_resource<R: ReplicatedComponent>(&self) -> bool {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.has_resource::<R>(),
            WorldServerImpl::Pipelined(ps) => ps.has_resource::<R>(),
        }
    }

    /// The hidden entity carrying resource `R`, or `None`.
    pub fn resource_entity<R: ReplicatedComponent>(&self) -> Option<E> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.resource_entity::<R>(),
            WorldServerImpl::Pipelined(ps) => ps.resource_entity::<R>(),
        }
    }

    /// Is `world_entity` a hidden resource entity?
    pub fn is_resource_entity(&self, world_entity: &E) -> bool {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.is_resource_entity(world_entity),
            WorldServerImpl::Pipelined(ps) => ps.is_resource_entity(world_entity),
        }
    }

    /// Number of currently-inserted resources.
    pub fn resources_count(&self) -> usize {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.resources_count(),
            WorldServerImpl::Pipelined(ps) => ps.resources_count(),
        }
    }

    /// Server-side authority status for resource `R`, or `None`.
    pub fn resource_authority_status<R: ReplicatedComponent>(&self) -> Option<EntityAuthStatus> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.resource_authority_status::<R>(),
            WorldServerImpl::Pipelined(ps) => ps.resource_authority_status::<R>(),
        }
    }

    /// Whether a user exists for the given key.
    pub fn user_exists(&self, user_key: &UserKey) -> bool {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.user_exists(user_key),
            WorldServerImpl::Pipelined(ps) => ps.user_exists(user_key),
        }
    }

    /// Keys of all currently-connected users.
    pub fn user_keys(&self) -> Vec<UserKey> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.user_keys(),
            WorldServerImpl::Pipelined(ps) => ps.user_keys(),
        }
    }

    /// Number of users currently tracked.
    pub fn users_count(&self) -> usize {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.users_count(),
            WorldServerImpl::Pipelined(ps) => ps.users_count(),
        }
    }

    /// Number of fully-connected users.
    pub fn user_count(&self) -> usize {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.user_count(),
            WorldServerImpl::Pipelined(ps) => ps.user_count(),
        }
    }

    /// Total number of replicated entities.
    pub fn entity_count(&self) -> usize {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.entity_count(),
            WorldServerImpl::Pipelined(ps) => ps.entity_count(),
        }
    }

    /// Whether a room exists for the given key.
    pub fn room_exists(&self, room_key: &RoomKey) -> bool {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.room_exists(room_key),
            WorldServerImpl::Pipelined(ps) => ps.room_exists(room_key),
        }
    }

    /// Keys of all rooms.
    pub fn room_keys(&self) -> Vec<RoomKey> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.room_keys(),
            WorldServerImpl::Pipelined(ps) => ps.room_keys(),
        }
    }

    /// Number of rooms.
    pub fn rooms_count(&self) -> usize {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.rooms_count(),
            WorldServerImpl::Pipelined(ps) => ps.rooms_count(),
        }
    }

    /// Number of rooms (alias).
    pub fn room_count(&self) -> usize {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.room_count(),
            WorldServerImpl::Pipelined(ps) => ps.room_count(),
        }
    }

    /// The pending incremental scope-check tuples.
    pub fn scope_checks_pending(&self) -> Vec<(RoomKey, UserKey, E)> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.scope_checks_pending(),
            WorldServerImpl::Pipelined(ps) => ps.scope_checks_pending(),
        }
    }

    /// Average jitter for the given user's client.
    pub fn jitter(&self, user_key: &UserKey) -> Option<f32> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.jitter(user_key),
            WorldServerImpl::Pipelined(ps) => ps.jitter(user_key),
        }
    }

    /// Average round-trip time for the given user's client.
    pub fn rtt(&self, user_key: &UserKey) -> Option<f32> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.rtt(user_key),
            WorldServerImpl::Pipelined(ps) => ps.rtt(user_key),
        }
    }

    /// Per-connection diagnostics for the given user.
    pub fn connection_stats(&self, user_key: &UserKey) -> Option<ConnectionStats> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.connection_stats(user_key),
            WorldServerImpl::Pipelined(ps) => ps.connection_stats(user_key),
        }
    }

    /// Whether the user's canonical send-side connection is materialized.
    pub fn user_connection_ready(&self, user_key: &UserKey) -> bool {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.user_connection_ready(user_key),
            WorldServerImpl::Pipelined(ps) => ps.user_connection_ready(user_key),
        }
    }

    /// Read-only reference to the Historian, or `None` if not enabled.
    pub fn historian(&self) -> Option<&Historian> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.historian(),
            WorldServerImpl::Pipelined(ps) => ps.historian(),
        }
    }

    // ── Mutations / drives ───────────────────────────────────────────────

    /// Maintain connections and read all inbound packet data.
    pub fn receive_all_packets(&mut self) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.receive_all_packets(),
            WorldServerImpl::Pipelined(ps) => {
                ps.with_monolithic_world_server(|ws| ws.receive_all_packets())
            }
        }
    }

    /// Decode and apply all buffered inbound packets to `world`.
    pub fn process_all_packets<W: WorldMutType<E>>(&mut self, mut world: W, now: &Instant) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.process_all_packets(&mut world, now),
            WorldServerImpl::Pipelined(ps) => {
                ps.with_monolithic_world_server(|ws| ws.process_all_packets(&mut world, now))
            }
        }
    }

    /// Drain and return all pending world events.
    pub fn take_world_events(&mut self) -> WorldEvents<E> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.take_world_events(),
            WorldServerImpl::Pipelined(ps) => {
                ps.with_monolithic_world_server(|ws| ws.take_world_events())
            }
        }
    }

    /// Advance the tick clock and return any new tick events.
    pub fn take_tick_events(&mut self, now: &Instant) -> TickEvents {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.take_tick_events(now),
            WorldServerImpl::Pipelined(ps) => {
                ps.with_monolithic_world_server(|ws| ws.take_tick_events(now))
            }
        }
    }

    /// Flush one tick's worth of outbound traffic against `world`.
    pub fn send_all_packets<W: WorldRefType<E> + Sync>(&mut self, world: W) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.send_all_packets(world),
            WorldServerImpl::Pipelined(ps) => {
                ps.with_monolithic_world_server(|ws| ws.send_all_packets(world))
            }
        }
    }

    /// Queue a message to the given user.
    pub fn send_message<C: Channel, M: Message>(
        &mut self,
        user_key: &UserKey,
        message: &M,
    ) -> Result<(), NaiaServerError> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.send_message::<C, M>(user_key, message),
            WorldServerImpl::Pipelined(ps) => ps.send_message::<C, M>(user_key, message),
        }
    }

    /// Broadcast a message to all connected users.
    pub fn broadcast_message<C: Channel, M: Message>(&mut self, message: &M) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.broadcast_message::<C, M>(message),
            WorldServerImpl::Pipelined(ps) => ps.broadcast_message::<C, M>(message),
        }
    }

    /// Drain all tick-buffered messages for the given tick.
    pub fn receive_tick_buffer_messages(&mut self, tick: &Tick) -> TickBufferMessages {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.receive_tick_buffer_messages(tick),
            WorldServerImpl::Pipelined(ps) => {
                ps.with_monolithic_world_server(|ws| ws.receive_tick_buffer_messages(tick))
            }
        }
    }

    /// Send a typed request to the given user.
    pub fn send_request<C: Channel, Q: Request>(
        &mut self,
        user_key: &UserKey,
        request: &Q,
    ) -> Result<ResponseReceiveKey<Q::Response>, NaiaServerError> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.send_request::<C, Q>(user_key, request),
            WorldServerImpl::Pipelined(ps) => ps.send_request::<C, Q>(user_key, request),
        }
    }

    /// Send a response for a given request.
    pub fn send_response<S: Response>(
        &mut self,
        response_key: &ResponseSendKey<S>,
        response: &S,
    ) -> bool {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.send_response::<S>(response_key, response),
            WorldServerImpl::Pipelined(ps) => ps.send_response::<S>(response_key, response),
        }
    }

    /// Outcome-reporting form of [`Self::send_response`] — see [`ResponseSendOutcome`].
    pub fn try_send_response<S: Response>(
        &mut self,
        response_key: &ResponseSendKey<S>,
        response: &S,
    ) -> ResponseSendOutcome {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.try_send_response::<S>(response_key, response),
            WorldServerImpl::Pipelined(ps) => ps.try_send_response::<S>(response_key, response),
        }
    }

    /// Poll for a response to a previously-sent request.
    pub fn receive_response<S: Response>(
        &mut self,
        response_key: &ResponseReceiveKey<S>,
    ) -> Option<(UserKey, S)> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.receive_response::<S>(response_key),
            WorldServerImpl::Pipelined(ps) => {
                ps.with_monolithic_world_server(|ws| ws.receive_response::<S>(response_key))
            }
        }
    }

    /// Clear the pending scope-check queue.
    pub fn mark_scope_checks_pending_handled(&mut self) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.mark_scope_checks_pending_handled(),
            WorldServerImpl::Pipelined(ps) => ps.mark_scope_checks_pending_handled(),
        }
    }

    /// Re-enqueue all current scope-check tuples.
    pub fn mark_all_scope_checks_pending(&mut self) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.mark_all_scope_checks_pending(),
            WorldServerImpl::Pipelined(ps) => ps.mark_all_scope_checks_pending(),
        }
    }

    /// Apply a new [`ReplicationConfig`] to an entity.
    pub fn configure_entity_replication<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        world_entity: &E,
        config: ReplicationConfig,
    ) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => {
                ws.configure_entity_replication(world, world_entity, config)
            }
            WorldServerImpl::Pipelined(ps) => {
                ps.configure_entity_replication(world_entity, config);
                ps.apply_pending_world_hooks(world);
            }
        }
    }

    /// Mark an entity as static.
    pub fn mark_entity_as_static(&mut self, world_entity: &E) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.mark_entity_as_static(world_entity),
            WorldServerImpl::Pipelined(ps) => ps.mark_entity_as_static(world_entity),
        }
    }

    /// Register a component insertion without touching the world.
    pub fn insert_component_worldless(&mut self, world_entity: &E, component: &mut dyn Replicate) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.insert_component_worldless(world_entity, component),
            WorldServerImpl::Pipelined(ps) => {
                ps.insert_component_worldless(world_entity, component)
            }
        }
    }

    /// Remove a component from the replication layer without touching the world.
    pub fn remove_component_worldless(&mut self, world_entity: &E, component_kind: &ComponentKind) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => {
                ws.remove_component_worldless(world_entity, component_kind)
            }
            WorldServerImpl::Pipelined(ps) => {
                ps.remove_component_worldless(world_entity, component_kind)
            }
        }
    }

    /// Remove an entity from all replication state without touching the world.
    pub fn despawn_entity_worldless(&mut self, world_entity: &E) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.despawn_entity_worldless(world_entity),
            WorldServerImpl::Pipelined(ps) => ps.despawn_entity_worldless(world_entity),
        }
    }

    /// Insert a Replicated Resource.
    pub fn insert_resource<W: WorldMutType<E>, R: ReplicatedComponent>(
        &mut self,
        world: W,
        value: R,
        is_static: bool,
    ) -> Result<E, ResourceAlreadyExists> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.insert_resource(world, value, is_static),
            WorldServerImpl::Pipelined(ps) => ps.insert_resource(world, value, is_static),
        }
    }

    /// Remove the resource of type `R` if present.
    pub fn remove_resource<W: WorldMutType<E>, R: ReplicatedComponent>(
        &mut self,
        world: W,
    ) -> bool {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.remove_resource::<W, R>(world),
            WorldServerImpl::Pipelined(ps) => ps.remove_resource::<W, R>(world),
        }
    }

    /// Register an already-spawned entity for replication.
    pub fn enable_entity_replication(&mut self, world_entity: &E) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.enable_entity_replication(world_entity),
            WorldServerImpl::Pipelined(ps) => ps.enable_entity_replication(world_entity),
        }
    }

    /// Disable replication for an entity.
    pub fn disable_entity_replication(&mut self, world_entity: &E) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.disable_entity_replication(world_entity),
            WorldServerImpl::Pipelined(ps) => ps.disable_entity_replication(world_entity),
        }
    }

    /// Pause replication for an entity.
    pub fn pause_entity_replication(&mut self, world_entity: &E) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.pause_entity_replication(world_entity),
            WorldServerImpl::Pipelined(ps) => ps.pause_entity_replication(world_entity),
        }
    }

    /// Resume replication for an entity.
    pub fn resume_entity_replication(&mut self, world_entity: &E) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.resume_entity_replication(world_entity),
            WorldServerImpl::Pipelined(ps) => ps.resume_entity_replication(world_entity),
        }
    }

    /// The current [`ReplicationConfig`] for an entity.
    pub fn entity_replication_config(&self, world_entity: &E) -> Option<ReplicationConfig> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.entity_replication_config(world_entity),
            WorldServerImpl::Pipelined(ps) => ps.entity_replication_config(world_entity),
        }
    }

    /// Server takes authority over an entity.
    pub fn entity_take_authority(&mut self, world_entity: &E) -> Result<(), AuthorityError> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.entity_take_authority(world_entity),
            WorldServerImpl::Pipelined(ps) => ps.entity_take_authority(world_entity),
        }
    }

    /// Server grants authority over an entity to a user.
    pub fn entity_give_authority(
        &mut self,
        user_key: &UserKey,
        world_entity: &E,
    ) -> Result<(), AuthorityError> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.entity_give_authority(user_key, world_entity),
            WorldServerImpl::Pipelined(ps) => ps.entity_give_authority(user_key, world_entity),
        }
    }

    /// Enable the per-tick lag-compensation snapshot buffer.
    pub fn enable_historian(&mut self, max_ticks: u16) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.enable_historian(max_ticks),
            WorldServerImpl::Pipelined(ps) => ps.enable_historian(max_ticks),
        }
    }

    /// Enable the lag-compensation snapshot buffer, snapshotting only the given
    /// component kinds.
    pub fn enable_historian_filtered(
        &mut self,
        max_ticks: u16,
        filter: impl IntoIterator<Item = ComponentKind>,
    ) {
        let filter: Vec<ComponentKind> = filter.into_iter().collect();
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.enable_historian_filtered(max_ticks, filter),
            WorldServerImpl::Pipelined(ps) => ps.enable_historian_filtered(max_ticks, filter),
        }
    }

    /// Record a Historian snapshot of all replicated component values.
    pub fn record_historian_tick<W: WorldRefType<E>>(&mut self, world: W, tick: Tick) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.record_historian_tick(world, tick),
            WorldServerImpl::Pipelined(ps) => ps.record_historian_tick(world, tick),
        }
    }

    // ── Entity ↔ GlobalEntity conversion ─────────────────────────────────

    /// Convert a [`GlobalEntity`] to the consumer's entity handle.
    pub fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<E, EntityDoesNotExistError> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.global_entity_to_entity(global_entity),
            WorldServerImpl::Pipelined(ps) => ps.global_entity_to_entity(global_entity),
        }
    }

    /// Convert the consumer's entity handle to a [`GlobalEntity`].
    pub fn entity_to_global_entity(
        &self,
        entity: &E,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.entity_to_global_entity(entity),
            WorldServerImpl::Pipelined(ps) => ps.entity_to_global_entity(entity),
        }
    }

    // ── G-unify P1: remaining NaiaServer-delegated surface ───────────────

    /// Register a user (post-handshake) with the world/replication layer.
    pub fn receive_user(&mut self, user_key: UserKey, user_address: SocketAddr) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.receive_user(user_key, user_address),
            WorldServerImpl::Pipelined(ps) => ps.receive_user(user_key, user_address),
        }
    }

    /// Queue a verified-handshake disconnect for the given user.
    pub fn user_queue_disconnect(&mut self, user_key: &UserKey, reason: DisconnectReason) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.user_queue_disconnect(user_key, reason),
            WorldServerImpl::Pipelined(ps) => ps.user_queue_disconnect(user_key, reason),
        }
    }

    /// Register an already-spawned entity as a static (immutable) entity.
    pub fn enable_static_entity_replication(&mut self, entity: &E) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.enable_static_entity_replication(entity),
            WorldServerImpl::Pipelined(ps) => ps.enable_static_entity_replication(entity),
        }
    }

    /// Whether the entity was spawned as static.
    pub fn entity_is_static(&self, world_entity: &E) -> bool {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.entity_is_static(world_entity),
            WorldServerImpl::Pipelined(ps) => ps.entity_is_static(world_entity),
        }
    }

    /// Whether the entity's replication config is `Delegated`.
    pub fn entity_is_delegated(&self, world_entity: &E) -> bool {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.entity_is_delegated(world_entity),
            WorldServerImpl::Pipelined(ps) => ps.entity_is_delegated(world_entity),
        }
    }

    /// Server releases authority back to `Available` (without revoking from a
    /// specific client unless `origin_user` is given).
    pub fn entity_release_authority(
        &mut self,
        origin_user: Option<&UserKey>,
        world_entity: &E,
    ) -> Result<(), AuthorityError> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.entity_release_authority(origin_user, world_entity),
            WorldServerImpl::Pipelined(ps) => {
                ps.entity_release_authority(origin_user, world_entity)
            }
        }
    }

    /// Switch a `Public` server entity to `Delegated`, enabling client authority
    /// requests. Returns `true` on success.
    pub fn enable_delegation<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        world_entity: &E,
    ) -> bool {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.enable_delegation(world, world_entity),
            WorldServerImpl::Pipelined(ps) => ps.enable_delegation(world, world_entity),
        }
    }

    /// All entities currently present in `world` (mode-agnostic).
    pub fn entities<W: WorldRefType<E>>(&self, world: W) -> Vec<E> {
        world.entities()
    }

    /// Read-only handle to the per-resource priority state, or `None` if `R` is
    /// not currently inserted. Composed from the unified resource/priority ops.
    pub fn resource_priority<R: ReplicatedComponent>(&self) -> Option<EntityPriorityRef<'_, E>> {
        let entity = self.resource_entity::<R>()?;
        Some(self.global_entity_priority(entity))
    }

    /// Mutable handle to the per-resource priority state, or `None` if `R` is
    /// not currently inserted.
    pub fn resource_priority_mut<R: ReplicatedComponent>(
        &mut self,
    ) -> Option<EntityPriorityMut<'_, E>> {
        let entity = self.resource_entity::<R>()?;
        Some(self.global_entity_priority_mut(entity))
    }

    // ── Diagnostics / bandwidth ──────────────────────────────────────────

    /// Rolling-average outgoing bandwidth (bytes/sec) across all clients.
    pub fn outgoing_bandwidth_total(&self) -> f32 {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.outgoing_bandwidth_total(),
            WorldServerImpl::Pipelined(ps) => ps.outgoing_bandwidth_total(),
        }
    }

    /// Bytes sent during the most recent `send_all_packets` tick.
    pub fn outgoing_bytes_last_tick(&self) -> u64 {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.outgoing_bytes_last_tick(),
            WorldServerImpl::Pipelined(ps) => ps.outgoing_bytes_last_tick(),
        }
    }

    /// Rolling-average incoming bandwidth (bytes/sec) across all clients.
    pub fn incoming_bandwidth_total(&self) -> f32 {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.incoming_bandwidth_total(),
            WorldServerImpl::Pipelined(ps) => ps.incoming_bandwidth_total(),
        }
    }

    /// Rolling-average outgoing bandwidth (bytes/sec) to one client address.
    pub fn outgoing_bandwidth_to_client(&self, address: &SocketAddr) -> f32 {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.outgoing_bandwidth_to_client(address),
            WorldServerImpl::Pipelined(ps) => ps.outgoing_bandwidth_to_client(address),
        }
    }

    /// Rolling-average incoming bandwidth (bytes/sec) from one client address.
    pub fn incoming_bandwidth_from_client(&self, address: &SocketAddr) -> f32 {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.incoming_bandwidth_from_client(address),
            WorldServerImpl::Pipelined(ps) => ps.incoming_bandwidth_from_client(address),
        }
    }

    // ── L3 send-state seam (lagged transmit) ─────────────────────────────

    /// Build the self-contained per-user [`SendPlan`] at the freeze point.
    pub fn prepare_send_job<W: WorldRefType<E> + Sync>(&mut self, world: &W) -> SendPlan {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.prepare_send_job(world),
            WorldServerImpl::Pipelined(ps) => {
                ps.with_monolithic_world_server(|ws| ws.prepare_send_job(world))
            }
        }
    }

    /// Serialize + send a prepared [`SendPlan`] against the snapshot `world`.
    pub fn transmit_send_job<W: WorldRefType<E> + Sync>(&mut self, world: W, plan: SendPlan) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.transmit_send_job(world, plan),
            WorldServerImpl::Pipelined(ps) => {
                ps.with_monolithic_world_server(|ws| ws.transmit_send_job(world, plan))
            }
        }
    }

    /// Send-side ACK drain (worker-preamble equivalent).
    pub fn drain_all_acks(&mut self) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.drain_all_acks(),
            WorldServerImpl::Pipelined(ps) => {
                ps.with_monolithic_world_server(|ws| ws.drain_all_acks())
            }
        }
    }

    /// A [`SendStateView`] backed by this server's shared state (registry-free
    /// snapshot assembler).
    pub fn send_state_view(&self) -> SendStateView<E> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.send_state_view(),
            WorldServerImpl::Pipelined(ps) => ps.send_state_view(),
        }
    }

    // ── Borrow-returning builders (mode-dispatched, task #9) ─────────────
    //
    // The builders are ONE type over both engine shapes (the `EntityMut`
    // precedent): the Resident arm borrows the fused `InternalWorldServer`; the
    // Pipelined arm borrows this `PipelinedWorldServer` and each builder method
    // dispatches to a coord fast path, D-slot staging, or direct parked-slot
    // cache mutation. No method panics — except per-USER priority (see below),
    // whose backing layer is send-resident AND borrow-returning.

    /// A read-only handle to the given user. Panics if no user exists.
    pub fn user(&self, user_key: &UserKey) -> UserRef<'_, E> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.user(user_key),
            WorldServerImpl::Pipelined(ps) => {
                if !ps.user_exists(user_key) {
                    panic!("No User exists for given Key!");
                }
                UserRef::with_pipeline(ps, user_key)
            }
        }
    }

    /// A read-only handle to the given user if it exists.
    pub fn user_opt(&self, user_key: &UserKey) -> Option<UserRef<'_, E>> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.user_opt(user_key),
            WorldServerImpl::Pipelined(ps) => ps
                .user_exists(user_key)
                .then(|| UserRef::with_pipeline(ps, user_key)),
        }
    }

    /// A mutable handle to the given user. Panics if no user exists.
    pub fn user_mut(&mut self, user_key: &UserKey) -> UserMut<'_, E> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.user_mut(user_key),
            WorldServerImpl::Pipelined(ps) => {
                if !ps.user_exists(user_key) {
                    panic!("No User exists for given Key!");
                }
                UserMut::with_pipeline(ps, user_key)
            }
        }
    }

    /// A mutable handle to the given user if it exists.
    pub fn user_mut_opt(&mut self, user_key: &UserKey) -> Option<UserMut<'_, E>> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.user_mut_opt(user_key),
            WorldServerImpl::Pipelined(ps) => ps
                .user_exists(user_key)
                .then(|| UserMut::with_pipeline(ps, user_key)),
        }
    }

    /// A read-only scope handle for the given user. Panics if no user exists.
    pub fn user_scope(&self, user_key: &UserKey) -> UserScopeRef<'_, E> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.user_scope(user_key),
            WorldServerImpl::Pipelined(ps) => {
                if !ps.user_exists(user_key) {
                    panic!("No User exists for given Key!");
                }
                UserScopeRef::with_pipeline(ps, user_key)
            }
        }
    }

    /// A mutable scope handle for the given user. Panics if no user exists.
    pub fn user_scope_mut(&mut self, user_key: &UserKey) -> UserScopeMut<'_, E> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.user_scope_mut(user_key),
            WorldServerImpl::Pipelined(ps) => {
                if !ps.user_exists(user_key) {
                    panic!("No User exists for given Key!");
                }
                UserScopeMut::with_pipeline(ps, user_key)
            }
        }
    }

    /// A read-only global priority handle for an entity. The sender-wide
    /// priority layer is coord-resident, so this works in both modes.
    pub fn global_entity_priority(&self, entity: E) -> EntityPriorityRef<'_, E> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.global_entity_priority(entity),
            WorldServerImpl::Pipelined(ps) => ps.global_entity_priority(entity),
        }
    }

    /// A mutable global priority handle for an entity (coord-resident; both modes).
    pub fn global_entity_priority_mut(&mut self, entity: E) -> EntityPriorityMut<'_, E> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.global_entity_priority_mut(entity),
            WorldServerImpl::Pipelined(ps) => ps.global_entity_priority_mut(entity),
        }
    }

    /// A read-only **per-user** priority handle for an entity (both modes).
    ///
    /// On a pipelined server the read reflects this tick's pending coord-side
    /// staging writes (task #13); the live send-side accumulator is not coord-
    /// reachable across ticks, but reads never affect wire output.
    pub fn user_entity_priority(&self, user_key: &UserKey, entity: E) -> EntityPriorityRef<'_, E> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.user_entity_priority(user_key, entity),
            WorldServerImpl::Pipelined(ps) => ps.user_entity_priority(user_key, entity),
        }
    }

    /// A mutable **per-user** priority handle for an entity (both modes).
    ///
    /// Pipelined writes target the per-tick coord staging, drained into
    /// `send.user_priorities` at the next `send` (task #13) — byte-identical to
    /// the resident direct write.
    pub fn user_entity_priority_mut(
        &mut self,
        user_key: &UserKey,
        entity: E,
    ) -> EntityPriorityMut<'_, E> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.user_entity_priority_mut(user_key, entity),
            WorldServerImpl::Pipelined(ps) => ps.user_entity_priority_mut(user_key, entity),
        }
    }

    /// Create a new room and return a mutable handle (both modes).
    pub fn create_room(&mut self) -> RoomMut<'_, E> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.create_room(),
            WorldServerImpl::Pipelined(ps) => {
                let room_key = ps.create_room();
                RoomMut::with_pipeline(ps, &room_key)
            }
        }
    }

    /// A read-only handle to the given room. Panics if no room exists.
    pub fn room(&self, room_key: &RoomKey) -> RoomRef<'_, E> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.room(room_key),
            WorldServerImpl::Pipelined(ps) => {
                if !ps.room_exists(room_key) {
                    panic!("No Room exists for given Key!");
                }
                RoomRef::with_pipeline(ps, room_key)
            }
        }
    }

    /// A mutable handle to the given room. Panics if no room exists.
    pub fn room_mut(&mut self, room_key: &RoomKey) -> RoomMut<'_, E> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.room_mut(room_key),
            WorldServerImpl::Pipelined(ps) => {
                if !ps.room_exists(room_key) {
                    panic!("No Room exists for given Key!");
                }
                RoomMut::with_pipeline(ps, room_key)
            }
        }
    }

    /// Spawn a new entity, register it for replication, and return the imperative
    /// builder. Mirrors [`crate::Server::spawn_entity`].
    pub fn spawn_entity<W: WorldMutType<E>>(&mut self, mut world: W) -> EntityMut<'_, E, W> {
        let world_entity = world.spawn_entity();
        let target = match &mut self.inner {
            WorldServerImpl::Resident(ws) => {
                ws.enable_entity_replication(&world_entity);
                EntityMutTarget::Resident(ws)
            }
            WorldServerImpl::Pipelined(ps) => {
                ps.enable_entity_replication(&world_entity);
                EntityMutTarget::Pipelined(ps)
            }
        };
        EntityMut::with_target(target, world, &world_entity)
    }

    /// A read-only handle to `entity`. Panics if `entity` does not exist in
    /// `world` (matching the resident [`crate::Server::entity`] contract).
    pub fn entity<W: WorldRefType<E>>(&self, world: W, entity: &E) -> EntityRef<'_, E, W> {
        if !world.has_entity(entity) {
            panic!("No Entity exists for given Key!");
        }
        let target = match &self.inner {
            WorldServerImpl::Resident(ws) => EntityRefTarget::Resident(ws),
            WorldServerImpl::Pipelined(ps) => EntityRefTarget::Pipelined(ps),
        };
        EntityRef::with_target(target, world, entity)
    }
}

#[cfg(feature = "interior_visibility")]
impl<E: Copy + Eq + Hash + Send + Sync + 'static> WorldServer<E> {
    /// All LocalEntity ids replicated to the given user. Mirrors
    /// [`crate::Server::local_entities`].
    pub fn local_entities(&self, user_key: &UserKey) -> Vec<naia_shared::LocalEntity> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.local_entities(user_key),
            WorldServerImpl::Pipelined(ps) => ps.local_entities(user_key),
        }
    }

    /// A read-only handle to the entity identified by `local_entity` for the
    /// given user (see [`crate::EntityRef::local_entity`]). Works in both drive
    /// shapes: the user→local-entity resolution reads send-resident state
    /// (directly on the fused engine, via a slot-lock read on the pipelined arm),
    /// then delegates to the shared [`entity`](Self::entity) tail.
    pub fn local_entity<W: WorldRefType<E>>(
        &self,
        world: W,
        user_key: &UserKey,
        local_entity: &naia_shared::LocalEntity,
    ) -> Option<EntityRef<'_, E, W>> {
        let world_entity = match &self.inner {
            WorldServerImpl::Resident(ws) => ws.local_to_world_entity(user_key, local_entity),
            WorldServerImpl::Pipelined(ps) => ps.local_to_world_entity(user_key, local_entity),
        }?;
        if !world.has_entity(&world_entity) {
            return None;
        }
        Some(self.entity(world, &world_entity))
    }

    /// A mutable handle to the entity identified by `local_entity` for the given
    /// user. Works in both drive shapes (see [`local_entity`](Self::local_entity)).
    pub fn local_entity_mut<W: WorldMutType<E>>(
        &mut self,
        world: W,
        user_key: &UserKey,
        local_entity: &naia_shared::LocalEntity,
    ) -> Option<EntityMut<'_, E, W>> {
        let world_entity = match &self.inner {
            WorldServerImpl::Resident(ws) => ws.local_to_world_entity(user_key, local_entity),
            WorldServerImpl::Pipelined(ps) => ps.local_to_world_entity(user_key, local_entity),
        }?;
        if !world.has_entity(&world_entity) {
            return None;
        }
        Some(self.entity_mut(world, &world_entity))
    }
}

#[cfg(feature = "test_utils")]
impl<E: Copy + Eq + Hash + Send + Sync + 'static> WorldServer<E> {
    #[doc(hidden)]
    pub fn set_global_entity_counter_for_test(&mut self, value: u64) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.set_global_entity_counter_for_test(value),
            WorldServerImpl::Pipelined(ps) => {
                ps.with_monolithic_world_server(|ws| ws.set_global_entity_counter_for_test(value))
            }
        }
    }

    #[doc(hidden)]
    pub fn diff_handler_global_count(&self) -> usize {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.diff_handler_global_count(),
            WorldServerImpl::Pipelined(ps) => ps.diff_handler_global_count(),
        }
    }

    #[doc(hidden)]
    pub fn diff_handler_global_count_by_kind(
        &self,
    ) -> std::collections::HashMap<ComponentKind, usize> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.diff_handler_global_count_by_kind(),
            WorldServerImpl::Pipelined(ps) => ps.diff_handler_global_count_by_kind(),
        }
    }

    #[doc(hidden)]
    pub fn diff_handler_user_counts(&self) -> std::collections::HashMap<UserKey, usize> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.diff_handler_user_counts(),
            WorldServerImpl::Pipelined(ps) => ps.diff_handler_user_counts(),
        }
    }

    #[doc(hidden)]
    pub fn scope_change_queue_len(&self) -> usize {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.scope_change_queue_len(),
            WorldServerImpl::Pipelined(ps) => ps.scope_change_queue_len(),
        }
    }

    #[doc(hidden)]
    pub fn total_dirty_update_count(&self) -> usize {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.total_dirty_update_count(),
            WorldServerImpl::Pipelined(ps) => ps.total_dirty_update_count(),
        }
    }

    /// Pipelined-only test hook: the shared `RecvHandle` slot, so a test can take
    /// the handle out (while workers are parked) to inject/drain tick-buffer
    /// messages directly. Panics on a resident server.
    #[doc(hidden)]
    pub fn recv_slot(&self) -> std::sync::Arc<parking_lot::Mutex<Option<crate::RecvHandle<E>>>> {
        match &self.inner {
            WorldServerImpl::Pipelined(ps) => ps.recv_slot(),
            WorldServerImpl::Resident(_) => {
                panic!("WorldServer::recv_slot called on a resident server (pipelined-only)")
            }
        }
    }

    #[doc(hidden)]
    pub fn inject_tick_buffer_message<C: Channel, M: Message>(
        &mut self,
        user_key: &UserKey,
        host_tick: &Tick,
        message_tick: &Tick,
        message: &M,
    ) -> bool {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => {
                ws.inject_tick_buffer_message::<C, M>(user_key, host_tick, message_tick, message)
            }
            WorldServerImpl::Pipelined(ps) => ps.with_monolithic_world_server(|ws| {
                ws.inject_tick_buffer_message::<C, M>(user_key, host_tick, message_tick, message)
            }),
        }
    }
}

impl<E: Hash + Copy + Eq + Sync + Send + 'static> EntityAndGlobalEntityConverter<E>
    for WorldServer<E>
{
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<E, EntityDoesNotExistError> {
        WorldServer::global_entity_to_entity(self, global_entity)
    }

    fn entity_to_global_entity(&self, entity: &E) -> Result<GlobalEntity, EntityDoesNotExistError> {
        WorldServer::entity_to_global_entity(self, entity)
    }
}
