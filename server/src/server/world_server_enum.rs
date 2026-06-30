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

use std::{hash::Hash, time::Duration};

use naia_shared::{
    AuthorityError, Channel, ComponentKind, ConnectionStats, EntityAndGlobalEntityConverter,
    EntityAuthStatus, EntityDoesNotExistError, GlobalEntity, Instant, Message, Protocol, Replicate,
    ReplicatedComponent, Request, ResourceAlreadyExists, Response, ResponseReceiveKey,
    ResponseSendKey, Tick, WorldMutType, WorldRefType,
};

use crate::{
    events::{world_events::WorldEvents, TickEvents},
    world::entity_mut::{EntityMut, EntityMutTarget},
    EntityOwner, EntityPriorityMut, EntityPriorityRef, Historian, InternalWorldServer,
    NaiaServerError, PipelinedWorldServer, ReceiveOutput, ReplicationConfig, RoomKey, RoomMut,
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
                let (_auth_tx, _auth_rx, ps, pr) =
                    crate::transport::Socket::listen(socket.into());
                ws.io_load(ps, pr);
            }
            WorldServerImpl::Pipelined(ps) => ps.listen(socket),
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
    pub fn receive<W: WorldMutType<E>>(&mut self, mut world: W) -> Vec<ReceiveOutput<E>> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => vec![ws.receive_with_world(world)],
            WorldServerImpl::Pipelined(ps) => ps.receive(&mut world),
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
            WorldServerImpl::Pipelined(_) => panic!(
                "WorldServer::is_listening is not available on a pipelined server (state lives in the parked send/recv workers)"
            ),
        }
    }

    /// The current average tick duration of the server.
    pub fn average_tick_duration(&self) -> Duration {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.average_tick_duration(),
            WorldServerImpl::Pipelined(ps) => ps.average_tick_duration(),
        }
    }

    /// Server-side authority status for `entity`, or `None` if it does not
    /// exist in `world`.
    pub fn entity_authority_status<W: WorldRefType<E>>(
        &self,
        world: W,
        entity: &E,
    ) -> Option<EntityAuthStatus> {
        if !world.has_entity(entity) {
            return None;
        }
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.entity(world, entity).authority(),
            WorldServerImpl::Pipelined(ps) => ps.entity_authority_status(entity),
        }
    }

    /// The owner of `entity`.
    pub fn entity_owner<W: WorldRefType<E>>(&self, world: W, entity: &E) -> EntityOwner {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.entity(world, entity).owner(),
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
            WorldServerImpl::Pipelined(_) => panic!(
                "WorldServer::scope_checks_pending is not available on a pipelined server (state lives in the parked send/recv workers)"
            ),
        }
    }

    /// Average jitter for the given user's client.
    pub fn jitter(&self, user_key: &UserKey) -> Option<f32> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.jitter(user_key),
            WorldServerImpl::Pipelined(_) => panic!(
                "WorldServer::jitter is not available on a pipelined server (state lives in the parked send/recv workers)"
            ),
        }
    }

    /// Average round-trip time for the given user's client.
    pub fn rtt(&self, user_key: &UserKey) -> Option<f32> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.rtt(user_key),
            WorldServerImpl::Pipelined(_) => panic!(
                "WorldServer::rtt is not available on a pipelined server (state lives in the parked send/recv workers)"
            ),
        }
    }

    /// Per-connection diagnostics for the given user.
    pub fn connection_stats(&self, user_key: &UserKey) -> Option<ConnectionStats> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.connection_stats(user_key),
            WorldServerImpl::Pipelined(_) => panic!(
                "WorldServer::connection_stats is not available on a pipelined server (state lives in the parked send/recv workers)"
            ),
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
            WorldServerImpl::Pipelined(ps) => ps.with_world_server(|ws| ws.receive_all_packets()),
        }
    }

    /// Decode and apply all buffered inbound packets to `world`.
    pub fn process_all_packets<W: WorldMutType<E>>(&mut self, mut world: W, now: &Instant) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.process_all_packets(&mut world, now),
            WorldServerImpl::Pipelined(ps) => {
                ps.with_world_server(|ws| ws.process_all_packets(&mut world, now))
            }
        }
    }

    /// Drain and return all pending world events.
    pub fn take_world_events(&mut self) -> WorldEvents<E> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.take_world_events(),
            WorldServerImpl::Pipelined(ps) => ps.with_world_server(|ws| ws.take_world_events()),
        }
    }

    /// Advance the tick clock and return any new tick events.
    pub fn take_tick_events(&mut self, now: &Instant) -> TickEvents {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.take_tick_events(now),
            WorldServerImpl::Pipelined(ps) => ps.with_world_server(|ws| ws.take_tick_events(now)),
        }
    }

    /// Flush one tick's worth of outbound traffic against `world`.
    pub fn send_all_packets<W: WorldRefType<E> + Sync>(&mut self, world: W) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.send_all_packets(world),
            WorldServerImpl::Pipelined(ps) => ps.with_world_server(|ws| ws.send_all_packets(world)),
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
            WorldServerImpl::Pipelined(ps) => {
                ps.with_world_server(|ws| ws.send_message::<C, M>(user_key, message))
            }
        }
    }

    /// Broadcast a message to all connected users.
    pub fn broadcast_message<C: Channel, M: Message>(&mut self, message: &M) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.broadcast_message::<C, M>(message),
            WorldServerImpl::Pipelined(ps) => {
                ps.with_world_server(|ws| ws.broadcast_message::<C, M>(message))
            }
        }
    }

    /// Drain all tick-buffered messages for the given tick.
    pub fn receive_tick_buffer_messages(&mut self, tick: &Tick) -> TickBufferMessages {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.receive_tick_buffer_messages(tick),
            WorldServerImpl::Pipelined(ps) => {
                ps.with_world_server(|ws| ws.receive_tick_buffer_messages(tick))
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
            WorldServerImpl::Pipelined(ps) => {
                ps.with_world_server(|ws| ws.send_request::<C, Q>(user_key, request))
            }
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
            WorldServerImpl::Pipelined(ps) => {
                ps.with_world_server(|ws| ws.send_response::<S>(response_key, response))
            }
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
                ps.with_world_server(|ws| ws.receive_response::<S>(response_key))
            }
        }
    }

    /// Clear the pending scope-check queue.
    pub fn mark_scope_checks_pending_handled(&mut self) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.mark_scope_checks_pending_handled(),
            WorldServerImpl::Pipelined(ps) => {
                ps.with_world_server(|ws| ws.mark_scope_checks_pending_handled())
            }
        }
    }

    /// Re-enqueue all current scope-check tuples.
    pub fn mark_all_scope_checks_pending(&mut self) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.mark_all_scope_checks_pending(),
            WorldServerImpl::Pipelined(ps) => {
                ps.with_world_server(|ws| ws.mark_all_scope_checks_pending())
            }
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
                ps.with_world_server(|ws| ws.configure_entity_replication(world, world_entity, config))
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
                ps.with_world_server(|ws| ws.insert_component_worldless(world_entity, component))
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
                ps.with_world_server(|ws| ws.remove_component_worldless(world_entity, component_kind))
            }
        }
    }

    /// Remove an entity from all replication state without touching the world.
    pub fn despawn_entity_worldless(&mut self, world_entity: &E) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.despawn_entity_worldless(world_entity),
            WorldServerImpl::Pipelined(ps) => {
                ps.with_world_server(|ws| ws.despawn_entity_worldless(world_entity))
            }
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
            WorldServerImpl::Pipelined(ps) => {
                ps.with_world_server(|ws| ws.insert_resource(world, value, is_static))
            }
        }
    }

    /// Remove the resource of type `R` if present.
    pub fn remove_resource<W: WorldMutType<E>, R: ReplicatedComponent>(&mut self, world: W) -> bool {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.remove_resource::<W, R>(world),
            WorldServerImpl::Pipelined(ps) => {
                ps.with_world_server(|ws| ws.remove_resource::<W, R>(world))
            }
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
            WorldServerImpl::Pipelined(ps) => {
                ps.with_world_server(|ws| ws.disable_entity_replication(world_entity))
            }
        }
    }

    /// Pause replication for an entity.
    pub fn pause_entity_replication(&mut self, world_entity: &E) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.pause_entity_replication(world_entity),
            WorldServerImpl::Pipelined(ps) => {
                ps.with_world_server(|ws| ws.pause_entity_replication(world_entity))
            }
        }
    }

    /// Resume replication for an entity.
    pub fn resume_entity_replication(&mut self, world_entity: &E) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.resume_entity_replication(world_entity),
            WorldServerImpl::Pipelined(ps) => {
                ps.with_world_server(|ws| ws.resume_entity_replication(world_entity))
            }
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
            WorldServerImpl::Pipelined(ps) => {
                ps.with_world_server(|ws| ws.entity_take_authority(world_entity))
            }
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
            WorldServerImpl::Pipelined(ps) => {
                ps.with_world_server(|ws| ws.entity_give_authority(user_key, world_entity))
            }
        }
    }

    /// Enable the per-tick lag-compensation snapshot buffer.
    pub fn enable_historian(&mut self, max_ticks: u16) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.enable_historian(max_ticks),
            WorldServerImpl::Pipelined(ps) => {
                ps.with_world_server(|ws| ws.enable_historian(max_ticks))
            }
        }
    }

    /// Record a Historian snapshot of all replicated component values.
    pub fn record_historian_tick<W: WorldRefType<E>>(&mut self, world: W, tick: Tick) {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.record_historian_tick(world, tick),
            WorldServerImpl::Pipelined(ps) => {
                ps.with_world_server(|ws| ws.record_historian_tick(world, tick))
            }
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

    // ── Borrow-returning builders (Resident-only) ────────────────────────

    /// A read-only handle to the given user. Panics on a pipelined server.
    pub fn user(&self, user_key: &UserKey) -> UserRef<'_, E> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.user(user_key),
            WorldServerImpl::Pipelined(_) => panic!(
                "WorldServer::user (borrow-returning builder) is not available on a pipelined server; use the coord-only mutators instead"
            ),
        }
    }

    /// A read-only handle to the given user if it exists. Panics on a pipelined server.
    pub fn user_opt(&self, user_key: &UserKey) -> Option<UserRef<'_, E>> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.user_opt(user_key),
            WorldServerImpl::Pipelined(_) => panic!(
                "WorldServer::user_opt (borrow-returning builder) is not available on a pipelined server; use the coord-only mutators instead"
            ),
        }
    }

    /// A mutable handle to the given user. Panics on a pipelined server.
    pub fn user_mut(&mut self, user_key: &UserKey) -> UserMut<'_, E> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.user_mut(user_key),
            WorldServerImpl::Pipelined(_) => panic!(
                "WorldServer::user_mut (borrow-returning builder) is not available on a pipelined server; use the coord-only mutators instead"
            ),
        }
    }

    /// A mutable handle to the given user if it exists. Panics on a pipelined server.
    pub fn user_mut_opt(&mut self, user_key: &UserKey) -> Option<UserMut<'_, E>> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.user_mut_opt(user_key),
            WorldServerImpl::Pipelined(_) => panic!(
                "WorldServer::user_mut_opt (borrow-returning builder) is not available on a pipelined server; use the coord-only mutators instead"
            ),
        }
    }

    /// A read-only scope handle for the given user. Panics on a pipelined server.
    pub fn user_scope(&self, user_key: &UserKey) -> UserScopeRef<'_, E> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.user_scope(user_key),
            WorldServerImpl::Pipelined(_) => panic!(
                "WorldServer::user_scope (borrow-returning builder) is not available on a pipelined server; use the coord-only mutators instead"
            ),
        }
    }

    /// A mutable scope handle for the given user. Panics on a pipelined server.
    pub fn user_scope_mut(&mut self, user_key: &UserKey) -> UserScopeMut<'_, E> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.user_scope_mut(user_key),
            WorldServerImpl::Pipelined(_) => panic!(
                "WorldServer::user_scope_mut (borrow-returning builder) is not available on a pipelined server; use the coord-only mutators instead"
            ),
        }
    }

    /// A read-only global priority handle for an entity. Panics on a pipelined server.
    pub fn global_entity_priority(&self, entity: E) -> EntityPriorityRef<'_, E> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.global_entity_priority(entity),
            WorldServerImpl::Pipelined(_) => panic!(
                "WorldServer::global_entity_priority (borrow-returning builder) is not available on a pipelined server; use the coord-only mutators instead"
            ),
        }
    }

    /// A mutable global priority handle for an entity. Panics on a pipelined server.
    pub fn global_entity_priority_mut(&mut self, entity: E) -> EntityPriorityMut<'_, E> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.global_entity_priority_mut(entity),
            WorldServerImpl::Pipelined(_) => panic!(
                "WorldServer::global_entity_priority_mut (borrow-returning builder) is not available on a pipelined server; use the coord-only mutators instead"
            ),
        }
    }

    /// A read-only per-user priority handle for an entity. Panics on a pipelined server.
    pub fn user_entity_priority(&self, user_key: &UserKey, entity: E) -> EntityPriorityRef<'_, E> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.user_entity_priority(user_key, entity),
            WorldServerImpl::Pipelined(_) => panic!(
                "WorldServer::user_entity_priority (borrow-returning builder) is not available on a pipelined server; use the coord-only mutators instead"
            ),
        }
    }

    /// A mutable per-user priority handle for an entity. Panics on a pipelined server.
    pub fn user_entity_priority_mut(
        &mut self,
        user_key: &UserKey,
        entity: E,
    ) -> EntityPriorityMut<'_, E> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.user_entity_priority_mut(user_key, entity),
            WorldServerImpl::Pipelined(_) => panic!(
                "WorldServer::user_entity_priority_mut (borrow-returning builder) is not available on a pipelined server; use the coord-only mutators instead"
            ),
        }
    }

    /// Create a new room and return a mutable handle. Panics on a pipelined server.
    pub fn create_room(&mut self) -> RoomMut<'_, E> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.create_room(),
            WorldServerImpl::Pipelined(_) => panic!(
                "WorldServer::create_room (borrow-returning builder) is not available on a pipelined server; use the coord-only mutators instead"
            ),
        }
    }

    /// A read-only handle to the given room. Panics on a pipelined server.
    pub fn room(&self, room_key: &RoomKey) -> RoomRef<'_, E> {
        match &self.inner {
            WorldServerImpl::Resident(ws) => ws.room(room_key),
            WorldServerImpl::Pipelined(_) => panic!(
                "WorldServer::room (borrow-returning builder) is not available on a pipelined server; use the coord-only mutators instead"
            ),
        }
    }

    /// A mutable handle to the given room. Panics on a pipelined server.
    pub fn room_mut(&mut self, room_key: &RoomKey) -> RoomMut<'_, E> {
        match &mut self.inner {
            WorldServerImpl::Resident(ws) => ws.room_mut(room_key),
            WorldServerImpl::Pipelined(_) => panic!(
                "WorldServer::room_mut (borrow-returning builder) is not available on a pipelined server; use the coord-only mutators instead"
            ),
        }
    }
}
