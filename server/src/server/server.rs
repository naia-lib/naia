use std::{hash::Hash, net::SocketAddr, panic, time::Duration};

use naia_shared::{
    AuthorityError, Channel, ComponentKind, EntityAndGlobalEntityConverter, EntityAuthStatus,
    EntityDoesNotExistError, EntityPriorityMut, EntityPriorityRef, GlobalEntity, Instant, Message,
    Protocol, ProtocolId, Replicate, ReplicatedComponent, Request, Response, ResponseReceiveKey,
    ResponseSendKey, SocketConfig, Tick, WorldMutType, WorldRefType,
};

use crate::{
    connection::tick_buffer_messages::TickBufferMessages,
    events::main_events::WorldPacketEvent,
    server::{main_server::MainServer, world_server::WorldServer},
    transport::Socket,
    transport::{PacketChannel, PacketSender},
    world::{entity_mut::EntityMut, entity_ref::EntityRef},
    ConnectEvent, DisconnectEvent, EntityOwner, Events, MainEvents, NaiaServerError,
    ReplicationConfig, RoomKey, RoomMut, RoomRef, ServerConfig, TickEvents, UserKey, UserMut,
    UserRef, UserScopeMut, UserScopeRef,
};

/// The naia server — accepts connections, replicates entities, and routes
/// messages.
///
/// `E` is your world's entity key type (e.g. a `u32` or ECS `Entity`). It must
/// be `Copy + Eq + Hash + Send + Sync`.
///
/// # Minimal server loop
///
/// ```text
/// loop {
///     server.receive_all_packets();                      // 1. read UDP/WebRTC
///     server.process_all_packets(&mut world, &now);      // 2. decode + dispatch
///     for event in server.take_world_events() { ... }   // 3. handle events
///     for event in server.take_tick_events(&now) { ... } // 4. advance ticks
///     // mutate components here
///     server.send_all_packets(&world);                   // 5. flush outbound
/// }
/// ```
///
/// Steps 1–5 must run in this order every frame; skipping any step causes
/// missed events or stale replication state.
pub struct Server<E: Copy + Eq + Hash + Send + Sync> {
    main_server: MainServer,
    outstanding_main_events: MainEvents,
    world_server: WorldServer<E>,
    to_world_sender_opt: Option<Box<dyn PacketSender>>,
}

impl<E: Copy + Eq + Hash + Send + Sync> Server<E> {
    /// Creates a new server with the given config and protocol.
    ///
    /// Call [`listen`](Server::listen) before entering the main loop.
    pub fn new<P: Into<Protocol>>(server_config: ServerConfig, protocol: P) -> Self {
        let mut protocol: Protocol = protocol.into();
        protocol.lock();
        let protocol_id = protocol.protocol_id();
        Self::new_with_protocol_id(server_config, protocol, protocol_id)
    }

    /// Creates a new server with an explicit protocol ID.
    ///
    /// # Adapter use only
    ///
    /// Bevy and macroquad adapters use this to inject a pre-computed ID.
    /// Prefer [`new`](Server::new) in application code.
    pub fn new_with_protocol_id(
        server_config: ServerConfig,
        protocol: Protocol,
        protocol_id: ProtocolId,
    ) -> Self {
        Self {
            main_server: MainServer::new_with_protocol_id(
                server_config.clone(),
                protocol.clone(),
                protocol_id,
            ),
            outstanding_main_events: MainEvents::default(),
            world_server: WorldServer::new(server_config, protocol),
            to_world_sender_opt: None,
        }
    }

    /// Binds the server to the given socket and starts accepting connections.
    ///
    /// Must be called before [`receive_all_packets`](Server::receive_all_packets).
    /// Calling more than once replaces the previous socket binding.
    pub fn listen<S: Into<Box<dyn Socket>>>(&mut self, socket: S) {
        self.main_server.listen(socket);

        // load world io
        let world_io_sender = self.main_server.sender_cloned();
        let (to_world_sender, world_io_receiver) = PacketChannel::unbounded();
        self.to_world_sender_opt = Some(to_world_sender);
        self.world_server
            .io_load(world_io_sender, world_io_receiver);
    }

    /// Returns `true` if the server is bound and listening for connections.
    pub fn is_listening(&self) -> bool {
        self.main_server.is_listening()
    }

    /// Returns the socket configuration used when [`listen`](Server::listen)
    /// was called.
    pub fn socket_config(&self) -> &SocketConfig {
        self.main_server.socket_config()
    }

    /// Reads all pending packets from the socket.
    ///
    /// Must be called **first** in the server loop, before
    /// [`process_all_packets`](Server::process_all_packets). Handles
    /// connection handshakes, queues incoming data, and routes world packets
    /// to the replication layer.
    pub fn receive_all_packets(&mut self) {
        let mut main_events = self.main_server.receive();

        // handle connects
        for user_key in main_events.read::<ConnectEvent>() {
            let user_address = self.main_server.user_address(&user_key).unwrap();
            self.world_server.receive_user(user_key, user_address);
        }

        // handle queued disconnects (from verified disconnect handshake packets)
        for user_key in main_events.read::<crate::events::main_events::QueuedDisconnectEvent>() {
            self.world_server.user_queue_disconnect(&user_key);
        }

        // handle world packets
        let to_world_sender = self.to_world_sender_opt.as_mut().unwrap();
        for (_, addr, payload) in main_events.read::<WorldPacketEvent>() {
            if let Err(_e) = to_world_sender.send(&addr, &payload) {
                main_events.push_error(NaiaServerError::SendError(addr));
            }
        }

        self.outstanding_main_events.append(main_events);

        // Need to run this to maintain connection with all clients, and receive packets
        // until none left
        self.world_server.receive_all_packets();
    }

    /// Decodes all received packets and applies changes to the world.
    ///
    /// Must be called after [`receive_all_packets`](Server::receive_all_packets)
    /// and before [`take_world_events`](Server::take_world_events). Applies
    /// incoming component mutations from client-authoritative entities and
    /// queues the resulting [`Events`] for the next [`take_world_events`]
    /// call.
    ///
    /// [`Events`]: crate::Events
    pub fn process_all_packets<W: WorldMutType<E>>(&mut self, world: W, now: &Instant) {
        self.world_server.process_all_packets(world, now);
    }

    /// Drains and returns all accumulated world events since the last call.
    ///
    /// Must be called after [`process_all_packets`](Server::process_all_packets).
    /// The returned [`Events`] contains connection/disconnection events, entity
    /// spawn/despawn notifications, component updates, and message arrivals.
    /// Events are consumed on each call — not calling this causes the internal
    /// buffer to grow without bound.
    ///
    /// [`Events`]: crate::Events
    pub fn take_world_events(&mut self) -> Events<E> {
        let mut world_events = self.world_server.take_world_events();

        // handle disconnects
        {
            let mut disconnects = Vec::new();
            for (user_key, addr) in world_events.read::<DisconnectEvent>() {
                self.main_server.disconnect_user(&user_key);
                disconnects.push((user_key, addr));
            }
            // put back into world events
            for (user_key, addr) in disconnects {
                world_events.push_disconnection(&user_key, addr);
            }
        }

        // combine events
        let main_events = std::mem::take(&mut self.outstanding_main_events);
        Events::<E>::new(main_events, world_events)
    }

    /// Advances the tick clock and returns any tick-boundary events.
    ///
    /// Must be called after [`take_world_events`](Server::take_world_events).
    /// Returns a [`TickEvents`] that indicates which server ticks have elapsed
    /// since the last call. The tick counter drives all tick-synchronised
    /// logic; call this every frame even when no ticks have elapsed.
    ///
    /// [`TickEvents`]: crate::TickEvents
    pub fn take_tick_events(&mut self, now: &Instant) -> TickEvents {
        self.world_server.take_tick_events(now)
    }

    // Connection lifecycle ──────────────────────────────────────────────────

    /// Accepts an incoming connection request.
    ///
    /// Call this inside a [`ConnectEvent`] handler to complete the handshake
    /// and admit the user. Until this is called the user is pending and no
    /// replication occurs.
    ///
    /// [`ConnectEvent`]: crate::ConnectEvent
    pub fn accept_connection(&mut self, user_key: &UserKey) {
        self.main_server.accept_connection(user_key);
    }

    /// Rejects an incoming connection request.
    ///
    /// Call this inside a [`ConnectEvent`] handler to refuse the user. The
    /// client receives a rejection response and the handshake is terminated.
    ///
    /// [`ConnectEvent`]: crate::ConnectEvent
    pub fn reject_connection(&mut self, user_key: &UserKey) {
        self.main_server.reject_connection(user_key);
    }

    // Messaging ─────────────────────────────────────────────────────────────

    /// Queues a message to be sent to the given user on the next
    /// [`send_all_packets`](Server::send_all_packets) call.
    ///
    /// `C` is the channel type (controls ordering and reliability guarantees).
    /// `M` is the message type (must be registered in the [`Protocol`]).
    ///
    /// # Errors
    ///
    /// Returns [`NaiaServerError::UserNotFound`] if `user_key` does not
    /// correspond to a currently connected user.
    ///
    /// [`Protocol`]: naia_shared::Protocol
    pub fn send_message<C: Channel, M: Message>(&mut self, user_key: &UserKey, message: &M) -> Result<(), NaiaServerError> {
        self.world_server.send_message::<C, M>(user_key, message)
    }

    /// Queues a message to be sent to **all** connected users on the next
    /// [`send_all_packets`](Server::send_all_packets) call.
    ///
    /// `C` is the channel type; `M` is the message type. Users that connect
    /// after this call do not receive the message.
    pub fn broadcast_message<C: Channel, M: Message>(&mut self, message: &M) {
        self.world_server.broadcast_message::<C, M>(message);
    }

    /// Sends a request to the given user and returns a key for polling the
    /// response.
    ///
    /// Use [`receive_response`](Server::receive_response) with the returned
    /// key to collect the reply once the client sends it back.
    ///
    /// # Errors
    ///
    /// Returns [`NaiaServerError::UserNotFound`] if `user_key` is invalid.
    pub fn send_request<C: Channel, Q: Request>(
        &mut self,
        user_key: &UserKey,
        request: &Q,
    ) -> Result<ResponseReceiveKey<Q::Response>, NaiaServerError> {
        self.world_server.send_request::<C, Q>(user_key, request)
    }

    /// Sends a response to a client's request.
    ///
    /// `response_key` is obtained from the [`RequestEvent`] that delivered the
    /// client's original request. Returns `true` on success; `false` if the
    /// key is no longer valid (e.g. the client disconnected).
    ///
    /// [`RequestEvent`]: crate::events::RequestEvent
    pub fn send_response<S: Response>(
        &mut self,
        response_key: &ResponseSendKey<S>,
        response: &S,
    ) -> bool {
        self.world_server.send_response(response_key, response)
    }

    /// Polls for a response to a previously sent server request.
    ///
    /// `response_key` is the value returned by
    /// [`send_request`](Server::send_request). Returns `Some((user_key,
    /// response))` once the client replies, or `None` if the response has not
    /// yet arrived (or the key is invalid).
    pub fn receive_response<S: Response>(
        &mut self,
        response_key: &ResponseReceiveKey<S>,
    ) -> Option<(UserKey, S)> {
        self.world_server.receive_response(response_key)
    }
    //

    /// Returns all tick-buffered messages that arrived for the given tick.
    ///
    /// Clients send [`TickBuffered`] channel messages stamped with the client
    /// tick at which the input was recorded. The server delivers them here once
    /// the server tick matches. Call this inside a tick-event handler after
    /// [`take_tick_events`](Server::take_tick_events).
    ///
    /// [`TickBuffered`]: naia_shared::ChannelMode::TickBuffered
    pub fn receive_tick_buffer_messages(&mut self, tick: &Tick) -> TickBufferMessages {
        self.world_server.receive_tick_buffer_messages(tick)
    }

    // Scope management ──────────────────────────────────────────────────────

    /// Returns every `(room, user, entity)` triple that is eligible for scope
    /// evaluation.
    ///
    /// Use this to implement a custom scope callback: iterate the triples,
    /// then call [`user_scope_mut`](Server::user_scope_mut) to include or
    /// exclude each entity for the corresponding user. For most use cases
    /// [`scope_checks_pending`](Server::scope_checks_pending) is more
    /// efficient because it returns only the triples whose scope status has
    /// changed since the last evaluation.
    pub fn scope_checks_all(&self) -> Vec<(RoomKey, UserKey, E)> {
        self.world_server.scope_checks_all()
    }

    /// Returns `(room, user, entity)` triples whose scope status is dirty and
    /// needs re-evaluation.
    ///
    /// This is the incremental counterpart to
    /// [`scope_checks_all`](Server::scope_checks_all). After evaluating each
    /// triple and updating [`user_scope_mut`](Server::user_scope_mut), call
    /// [`mark_scope_checks_pending_handled`](Server::mark_scope_checks_pending_handled)
    /// to clear the dirty set.
    pub fn scope_checks_pending(&self) -> Vec<(RoomKey, UserKey, E)> {
        self.world_server.scope_checks_pending()
    }

    /// Clears the pending scope-check dirty set.
    ///
    /// Call this after processing all triples from
    /// [`scope_checks_pending`](Server::scope_checks_pending) to signal that
    /// the current batch has been handled.
    pub fn mark_scope_checks_pending_handled(&mut self) {
        self.world_server.mark_scope_checks_pending_handled();
    }

    /// Flushes all queued updates and messages to connected clients.
    ///
    /// Must be called **last** in the server loop, after all component
    /// mutations for the current frame have been applied. Computes diffs,
    /// serialises packets, and hands them to the transport layer. If this is
    /// not called, clients never receive any updates.
    pub fn send_all_packets<W: WorldRefType<E>>(&mut self, world: W) {
        self.world_server.send_all_packets(world);
    }

    // Entities ──────────────────────────────────────────────────────────────

    /// Spawns a new entity and returns a builder for configuring it.
    ///
    /// Call [`insert_component`](crate::EntityMut::insert_component) on the
    /// returned [`EntityMut`] to add components. Chain
    /// [`.as_static()`](crate::EntityMut::as_static) before inserting
    /// components to create a static (immutable) entity.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use naia_server::Server;
    /// # fn example<E: Copy + Eq + std::hash::Hash + Send + Sync, W: naia_shared::WorldMutType<E>>(server: &mut Server<E>, world: W, component: impl naia_shared::ReplicatedComponent) {
    /// // Dynamic entity (default — components are diff-tracked):
    /// server.spawn_entity(world).insert_component(component);
    /// # }
    /// ```
    pub fn spawn_entity<W: WorldMutType<E>>(&'_ mut self, world: W) -> EntityMut<'_, E, W> {
        self.world_server.spawn_entity(world)
    }

    /// Returns `true` if the entity was spawned as static.
    ///
    /// Static entities send a full component snapshot once when they enter a
    /// user's scope; they are never diff-tracked after that.
    pub fn entity_is_static(&self, world_entity: &E) -> bool {
        self.world_server.entity_is_static(world_entity)
    }

    /// Returns `true` if the entity's replication config has
    /// [`Publicity::Delegated`](naia_shared::Publicity::Delegated).
    ///
    /// Convenience predicate equivalent to:
    /// `server.entity_replication_config(e).map_or(false, |c| c.publicity.is_delegated())`
    pub fn entity_is_delegated(&self, world_entity: &E) -> bool {
        self.world_server.entity_is_delegated(world_entity)
    }

    // Replicated Resources -----------------------------------------------
    //
    // A Replicated Resource is a per-`World` singleton whose value is
    // server-replicated to all connected clients with diff-tracked,
    // per-field updates. Internally, a hidden 1-component entity carries
    // the resource value as its sole replicated component.
    //
    // Use `insert_resource(world, value, false)` for dynamic (diff-tracked)
    // resources and `insert_resource(world, value, true)` for static ones.
    //
    // See `_AGENTS/RESOURCES_PLAN.md`.

    /// Insert a Replicated Resource.
    /// `is_static = true` → no diff-tracking (immutable after insertion).
    /// `is_static = false` → delta-tracked; field changes are replicated.
    pub fn insert_resource<W: WorldMutType<E>, R: ReplicatedComponent>(
        &mut self,
        world: W,
        value: R,
        is_static: bool,
    ) -> Result<E, naia_shared::ResourceAlreadyExists> {
        self.world_server.insert_resource(world, value, is_static)
    }

    /// Remove the resource of type `R`. Returns `true` if a resource
    /// was removed; `false` if `R` was not present.
    pub fn remove_resource<W: WorldMutType<E>, R: ReplicatedComponent>(&mut self, world: W) -> bool {
        self.world_server.remove_resource::<W, R>(world)
    }

    /// True iff a resource of type `R` is currently inserted.
    pub fn has_resource<R: ReplicatedComponent>(&self) -> bool {
        self.world_server.has_resource::<R>()
    }

    /// O(1): the hidden world-entity carrying resource `R`, or `None`
    /// if `R` is not currently inserted. Mostly used by tests and the
    /// Bevy adapter; user code should normally read via `Res<R>` (in
    /// Bevy) or `server.resource::<R>(world)` (in core).
    pub fn resource_entity<R: ReplicatedComponent>(&self) -> Option<E> {
        self.world_server.resource_entity::<R>()
    }

    /// True iff `world_entity` is the hidden entity for any Replicated
    /// Resource. Used by Bevy adapter event-emission filter (D13).
    pub fn is_resource_entity(&self, world_entity: &E) -> bool {
        self.world_server.is_resource_entity(world_entity)
    }

    /// Number of currently-inserted resources.
    pub fn resources_count(&self) -> usize {
        self.world_server.resources_count()
    }

    /// Read-only access to the current value of resource `R`.
    /// Returns `None` if `R` is not currently inserted.
    ///
    /// This goes through the world's component storage; the result
    /// borrows from `world` for the lifetime of the call.
    pub fn resource<'w, R: ReplicatedComponent, W: WorldRefType<E> + 'w>(
        &self,
        world: &'w W,
    ) -> Option<naia_shared::ReplicaRefWrapper<'w, R>> {
        let entity = self.world_server.resource_entity::<R>()?;
        world.component::<R>(&entity)
    }

    /// Read-only handle to the per-resource priority state, or `None`
    /// if `R` is not currently inserted.
    pub fn resource_priority<R: ReplicatedComponent>(&self) -> Option<EntityPriorityRef<'_, E>> {
        self.world_server.resource_priority::<R>()
    }

    /// Mutable handle to the per-resource priority state. Returns `None`
    /// if `R` is not currently inserted. Set the per-tick gain via
    /// `.set_gain(f32)` or apply a one-shot bump via `.boost_once(f32)`.
    /// Default gain (no override) is 1.0.
    pub fn resource_priority_mut<R: ReplicatedComponent>(
        &mut self,
    ) -> Option<EntityPriorityMut<'_, E>> {
        self.world_server.resource_priority_mut::<R>()
    }

    /// Configure the replication mode of an inserted resource (e.g.
    /// `ReplicationConfig::delegated()` to make `R` client-delegable).
    /// Returns `true` if the resource is present and was reconfigured;
    /// `false` if `R` is not currently inserted.
    ///
    /// Per D2 / D3 of RESOURCES_PLAN: server-authoritative is the
    /// default; opt into delegation via this method (typically called
    /// immediately after `insert_resource`).
    pub fn configure_resource<W: WorldMutType<E>, R: ReplicatedComponent>(
        &mut self,
        world: &mut W,
        config: ReplicationConfig,
    ) -> bool {
        let Some(entity) = self.world_server.resource_entity::<R>() else {
            return false;
        };
        self.world_server
            .configure_entity_replication(world, &entity, config);
        true
    }

    /// Read the current authority status of resource `R` from the
    /// server's POV. `None` if `R` is not inserted, or if it is not a
    /// delegable resource.
    pub fn resource_authority_status<R: ReplicatedComponent>(
        &self,
    ) -> Option<EntityAuthStatus> {
        let entity = self.world_server.resource_entity::<R>()?;
        self.world_server.entity_authority_status(&entity)
    }

    /// Server takes authority back from whichever client (if any)
    /// currently holds it. Mirror of `entity_take_authority`.
    pub fn resource_take_authority<R: ReplicatedComponent>(
        &mut self,
    ) -> Result<(), AuthorityError> {
        let entity = self
            .world_server
            .resource_entity::<R>()
            .ok_or(AuthorityError::ResourceNotPresent)?;
        self.world_server.entity_take_authority(&entity)
    }

    /// Server releases its authority on resource `R` (sets status back
    /// to Available). Mirror of `entity_release_authority`. Returns
    /// `Err` if `R` is not inserted or not delegable.
    pub fn resource_release_authority<R: ReplicatedComponent>(
        &mut self,
    ) -> Result<(), AuthorityError> {
        let entity = self
            .world_server
            .resource_entity::<R>()
            .ok_or(AuthorityError::ResourceNotPresent)?;
        self.world_server.entity_release_authority(None, &entity)
    }

    /// Registers the entity with the replication layer.
    ///
    /// # Adapter use only
    ///
    /// Called by the Bevy adapter when a [`Replicate`] component is inserted
    /// via ECS commands. Do not call from application code — use
    /// [`spawn_entity`](Server::spawn_entity) instead.
    ///
    /// [`Replicate`]: naia_shared::Replicate
    pub fn enable_entity_replication(&mut self, entity: &E) {
        self.world_server.enable_entity_replication(entity);
    }

    /// Registers the entity as a static (immutable) entity with the
    /// replication layer.
    ///
    /// # Adapter use only
    ///
    /// Called by the Bevy adapter's `enable_static_replication` command. Do
    /// not call from application code — use
    /// `spawn_entity(world).as_static()` instead.
    pub fn enable_static_entity_replication(&mut self, entity: &E) {
        self.world_server.enable_static_entity_replication(entity);
    }

    /// Unregisters the entity from the replication layer and despawns it on
    /// all clients.
    ///
    /// # Adapter use only
    ///
    /// Called by the Bevy adapter when a [`Replicate`] component is removed.
    /// Do not call from application code.
    ///
    /// [`Replicate`]: naia_shared::Replicate
    pub fn disable_entity_replication(&mut self, world_entity: &E) {
        self.world_server.disable_entity_replication(world_entity);
    }

    /// Pauses replication for this entity without despawning it on clients.
    ///
    /// # Adapter use only
    ///
    /// Component changes will not be transmitted until
    /// [`resume_entity_replication`](Server::resume_entity_replication) is
    /// called. Used by Bevy adapter visibility systems.
    pub fn pause_entity_replication(&mut self, world_entity: &E) {
        self.world_server.pause_entity_replication(world_entity);
    }

    /// Resumes replication for an entity previously paused with
    /// [`pause_entity_replication`](Server::pause_entity_replication).
    ///
    /// # Adapter use only
    ///
    /// Used by Bevy adapter visibility systems.
    pub fn resume_entity_replication(&mut self, world_entity: &E) {
        self.world_server.resume_entity_replication(world_entity);
    }

    /// Returns the current [`ReplicationConfig`] for the entity, or `None` if
    /// the entity is not registered with the replication layer.
    ///
    /// # Adapter use only
    ///
    /// Prefer [`entity_is_static`](Server::entity_is_static) and
    /// [`entity_is_delegated`](Server::entity_is_delegated) in application
    /// code.
    pub fn entity_replication_config(&self, world_entity: &E) -> Option<ReplicationConfig> {
        self.world_server.entity_replication_config(world_entity)
    }

    /// Forces the server to reclaim authority over the entity, revoking any
    /// client grant in progress.
    ///
    /// # Adapter use only
    ///
    /// Application code should call
    /// [`entity_mut(...).take_authority()`](crate::EntityMut::take_authority)
    /// instead.
    pub fn entity_take_authority(&mut self, world_entity: &E) -> Result<(), AuthorityError> {
        self.world_server.entity_take_authority(world_entity)
    }

    /// Grants authority over the entity to the specified user.
    ///
    /// # Adapter use only
    ///
    /// Application code should call
    /// [`entity_mut(...).give_authority(user_key)`](crate::EntityMut::give_authority)
    /// instead.
    pub fn entity_give_authority(
        &mut self,
        origin_user: &UserKey,
        world_entity: &E,
    ) -> Result<(), AuthorityError> {
        self.world_server.entity_give_authority(origin_user, world_entity)
    }

    /// Updates the [`ReplicationConfig`] for a registered entity.
    ///
    /// Changes take effect on the next [`send_all_packets`](Server::send_all_packets)
    /// call. For example, switch from `Public` to `Delegated` to allow clients
    /// to request authority.
    pub fn configure_entity_replication<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        world_entity: &E,
        config: ReplicationConfig,
    ) {
        self.world_server
            .configure_entity_replication(world, world_entity, config);
    }

    /// Returns the current authority status for the entity from the server's
    /// perspective, or `None` if the entity is not delegable.
    ///
    /// # Adapter use only
    ///
    /// Application code should inspect authority via [`EntityRef`].
    ///
    /// [`EntityRef`]: crate::EntityRef
    pub fn entity_authority_status(&self, world_entity: &E) -> Option<EntityAuthStatus> {
        self.world_server.entity_authority_status(world_entity)
    }

    /// Releases authority back to the `Available` state without revoking from
    /// a specific client.
    ///
    /// # Adapter use only
    ///
    /// Application code should call
    /// [`entity_mut(...).release_authority()`](crate::EntityMut::release_authority)
    /// instead.
    pub fn entity_release_authority(
        &mut self,
        origin_user: Option<&UserKey>,
        world_entity: &E,
    ) -> Result<(), AuthorityError> {
        self.world_server
            .entity_release_authority(origin_user, world_entity)
    }

    /// Switches a `Public` server entity to `Delegated`, enabling clients to
    /// request authority over it.
    ///
    /// The entity must be server-owned and currently `Public`. Returns `true`
    /// on success; `false` if the preconditions are not met. This is a
    /// convenience wrapper around
    /// [`configure_entity_replication`](Server::configure_entity_replication)
    /// with [`ReplicationConfig::delegated()`].
    pub fn enable_delegation<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        world_entity: &E,
    ) -> bool {
        self.world_server.enable_delegation(world, world_entity)
    }

    /// Returns a read-only handle to the entity.
    ///
    /// # Panics
    ///
    /// Panics if the entity is not registered with the replication layer.
    pub fn entity<W: WorldRefType<E>>(&'_ self, world: W, entity: &E) -> EntityRef<'_, E, W> {
        self.world_server.entity(world, entity)
    }

    /// Returns a mutable handle to the entity.
    ///
    /// # Panics
    ///
    /// Panics if the entity is not registered with the replication layer.
    pub fn entity_mut<W: WorldMutType<E>>(
        &'_ mut self,
        world: W,
        entity: &E,
    ) -> EntityMut<'_, E, W> {
        self.world_server.entity_mut(world, entity)
    }

    /// Returns all entities currently registered with the replication layer.
    pub fn entities<W: WorldRefType<E>>(&self, world: W) -> Vec<E> {
        self.world_server.entities(world)
    }

    /// Returns the [`EntityOwner`] for the given entity.
    ///
    /// # Adapter use only
    ///
    /// Adapter crates use this to route authority-delegation events.
    /// Application code should inspect ownership via [`EntityRef`].
    ///
    /// [`EntityRef`]: crate::EntityRef
    pub fn entity_owner(&self, world_entity: &E) -> EntityOwner {
        self.world_server.entity_owner(world_entity)
    }

    // Users ─────────────────────────────────────────────────────────────────

    /// Returns `true` if the given user key corresponds to a currently
    /// connected user.
    pub fn user_exists(&self, user_key: &UserKey) -> bool {
        self.main_server.user_exists(user_key)
    }

    /// Returns a read-only handle to the user.
    ///
    /// # Panics
    ///
    /// Panics if the user does not exist.
    pub fn user(&'_ self, user_key: &UserKey) -> UserRef<'_, E> {
        if self.user_exists(user_key) {
            return UserRef::new(&self.world_server, user_key);
        }
        panic!("No User exists for given Key!");
    }

    /// Returns a mutable handle to the user.
    ///
    /// # Panics
    ///
    /// Panics if the user does not exist.
    pub fn user_mut(&'_ mut self, user_key: &UserKey) -> UserMut<'_, E> {
        if self.user_exists(user_key) {
            return UserMut::new(&mut self.world_server, user_key);
        }
        panic!("No User exists for given Key!");
    }

    /// Returns the keys of all currently connected users.
    pub fn user_keys(&self) -> Vec<UserKey> {
        self.main_server.user_keys()
    }

    /// Returns the number of currently connected users.
    pub fn users_count(&self) -> usize {
        self.main_server.users_count()
    }

    /// Returns the socket address of the user, or `None` if the user is not
    /// found or the handshake is not yet complete.
    pub fn user_address(&self, user_key: &UserKey) -> Option<std::net::SocketAddr> {
        self.main_server.user_address(user_key)
    }

    /// Returns a read-only view of the fine-grained scope for the given user.
    ///
    /// Use this to query whether a specific entity is currently included in
    /// the user's scope. For mutation use
    /// [`user_scope_mut`](Server::user_scope_mut).
    pub fn user_scope(&'_ self, user_key: &UserKey) -> UserScopeRef<'_, E> {
        self.world_server.user_scope(user_key)
    }

    /// Returns a mutable handle to the fine-grained scope for the given user.
    ///
    /// Call [`include`](crate::UserScopeMut::include) or
    /// [`exclude`](crate::UserScopeMut::exclude) to control which entities
    /// replicate to this user within their shared rooms.
    pub fn user_scope_mut(&'_ mut self, user_key: &UserKey) -> UserScopeMut<'_, E> {
        self.world_server.user_scope_mut(user_key)
    }

    // Priority ──────────────────────────────────────────────────────────────

    /// Returns the global (cross-user) priority state for the entity.
    ///
    /// Global priority affects how quickly this entity's updates are included
    /// in packets across all users. Adjust the gain or apply a boost via the
    /// returned handle.
    pub fn global_entity_priority(&self, entity: E) -> EntityPriorityRef<'_, E> {
        self.world_server.global_entity_priority(entity)
    }

    /// Returns a mutable handle to the global priority state for the entity.
    ///
    /// Use `.set_gain(f32)` to change the per-tick accumulation rate, or
    /// `.boost_once(f32)` to apply a one-shot priority spike.
    pub fn global_entity_priority_mut(&mut self, entity: E) -> EntityPriorityMut<'_, E> {
        self.world_server.global_entity_priority_mut(entity)
    }

    /// Returns the per-user priority state for the entity.
    ///
    /// Per-user priority overrides the global priority for a specific client,
    /// allowing differential update rates across users for the same entity.
    pub fn user_entity_priority(
        &self,
        user_key: &UserKey,
        entity: E,
    ) -> EntityPriorityRef<'_, E> {
        self.world_server.user_entity_priority(user_key, entity)
    }

    /// Returns a mutable handle to the per-user priority state for the entity.
    pub fn user_entity_priority_mut(
        &mut self,
        user_key: &UserKey,
        entity: E,
    ) -> EntityPriorityMut<'_, E> {
        self.world_server.user_entity_priority_mut(user_key, entity)
    }

    // Rooms ─────────────────────────────────────────────────────────────────

    /// Creates a new room and returns a mutable handle for configuring it.
    ///
    /// Rooms are the coarse scoping unit: a user and an entity must share at
    /// least one room before the fine-grained [`UserScope`] layer is
    /// consulted. Retrieve the [`RoomKey`] from the returned handle.
    ///
    /// [`UserScope`]: crate::UserScopeMut
    pub fn create_room(&'_ mut self) -> RoomMut<'_, E> {
        self.world_server.create_room()
    }

    /// Returns `true` if the given room key corresponds to an existing room.
    pub fn room_exists(&self, room_key: &RoomKey) -> bool {
        self.world_server.room_exists(room_key)
    }

    /// Returns a read-only handle to the room.
    ///
    /// # Panics
    ///
    /// Panics if the room does not exist.
    pub fn room(&'_ self, room_key: &RoomKey) -> RoomRef<'_, E> {
        self.world_server.room(room_key)
    }

    /// Returns a mutable handle to the room.
    ///
    /// # Panics
    ///
    /// Panics if the room does not exist.
    pub fn room_mut(&'_ mut self, room_key: &RoomKey) -> RoomMut<'_, E> {
        self.world_server.room_mut(room_key)
    }

    /// Returns the keys of all currently existing rooms.
    pub fn room_keys(&self) -> Vec<RoomKey> {
        self.world_server.room_keys()
    }

    /// Returns the number of currently existing rooms.
    pub fn rooms_count(&self) -> usize {
        self.world_server.rooms_count()
    }

    // Ticks ─────────────────────────────────────────────────────────────────

    /// Returns the server's current tick counter.
    ///
    /// This is the tick number used to stamp outgoing packets. It advances
    /// by one each time a tick interval elapses, as tracked by
    /// [`take_tick_events`](Server::take_tick_events).
    pub fn current_tick(&self) -> Tick {
        self.world_server.current_tick()
    }

    /// Returns the rolling-average duration of a server tick.
    ///
    /// Useful for monitoring whether the server is keeping up with the
    /// configured tick interval.
    pub fn average_tick_duration(&self) -> Duration {
        self.world_server.average_tick_duration()
    }

    // Diagnostics ───────────────────────────────────────────────────────────

    /// Returns the rolling-average outgoing bandwidth (bytes/second) across
    /// all connected clients.
    pub fn outgoing_bandwidth_total(&self) -> f32 {
        self.world_server.outgoing_bandwidth_total()
    }

    /// Bytes sent during the most recent `send_all_packets` tick. Precise
    /// per-tick counter (unlike the rolling-window `outgoing_bandwidth_total`).
    /// Zero before the first tick; read after a tick has run.
    pub fn outgoing_bytes_last_tick(&self) -> u64 {
        self.world_server.outgoing_bytes_last_tick()
    }

    /// Returns the rolling-average incoming bandwidth (bytes/second) across
    /// all connected clients.
    pub fn incoming_bandwidth_total(&self) -> f32 {
        self.world_server.incoming_bandwidth_total()
    }

    /// Returns the rolling-average outgoing bandwidth (bytes/second) to the
    /// given client address.
    pub fn outgoing_bandwidth_to_client(&self, address: &SocketAddr) -> f32 {
        self.world_server.outgoing_bandwidth_to_client(address)
    }

    /// Returns the rolling-average incoming bandwidth (bytes/second) from the
    /// given client address.
    pub fn incoming_bandwidth_from_client(&self, address: &SocketAddr) -> f32 {
        self.world_server.incoming_bandwidth_from_client(address)
    }

    /// Returns the average round-trip time (milliseconds) to the given user's
    /// client, or `None` if not yet measured.
    pub fn rtt(&self, user_key: &UserKey) -> Option<f32> {
        self.world_server.rtt(user_key)
    }

    /// Returns the average jitter (milliseconds) measured for the given user's
    /// connection, or `None` if not yet measured.
    pub fn jitter(&self, user_key: &UserKey) -> Option<f32> {
        self.world_server.jitter(user_key)
    }

    /// Despawns the entity from the replication layer without touching the world.
    ///
    /// # Adapter use only
    ///
    /// The Bevy adapter calls this when the ECS world has already removed the
    /// entity. Application code should use the world's own despawn path, which
    /// triggers the adapter hook automatically.
    pub fn despawn_entity_worldless(&mut self, world_entity: &E) {
        self.world_server.despawn_entity_worldless(world_entity);
    }

    /// Registers a component insertion with the replication layer without
    /// touching the world's component storage.
    ///
    /// # Adapter use only
    ///
    /// The Bevy adapter calls this when the component already exists in the
    /// ECS world and only the replication bookkeeping needs updating.
    pub fn insert_component_worldless(&mut self, world_entity: &E, component: &mut dyn Replicate) {
        self.world_server
            .insert_component_worldless(world_entity, component);
    }

    /// Registers a component removal with the replication layer without
    /// touching the world's component storage.
    ///
    /// # Adapter use only
    ///
    /// The Bevy adapter calls this when the component has already been removed
    /// from the ECS world and only the replication bookkeeping needs updating.
    pub fn remove_component_worldless(&mut self, world_entity: &E, component_kind: &ComponentKind) {
        self.world_server
            .remove_component_worldless(world_entity, component_kind);
    }
}

impl<E: Hash + Copy + Eq + Sync + Send> EntityAndGlobalEntityConverter<E> for Server<E> {
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<E, EntityDoesNotExistError> {
        self.world_server.global_entity_to_entity(global_entity)
    }

    fn entity_to_global_entity(
        &self,
        world_entity: &E,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.world_server.entity_to_global_entity(world_entity)
    }
}

cfg_if! {
    if #[cfg(feature = "interior_visibility")] {

        use naia_shared::LocalEntity;

        impl<E: Copy + Eq + Hash + Send + Sync> Server<E> {
            /// Returns all LocalEntity IDs for entities replicated to the given user.
            ///
            /// Returns the set of LocalEntity IDs that currently exist for that user
            /// (i.e., all entities replicated to that user).
            /// The ordering doesn't matter.
            ///
            /// # Panics
            ///
            /// Panics if the user does not exist.
            pub fn local_entities(&self, user_key: &UserKey) -> Vec<LocalEntity> {
                self.world_server.local_entities(user_key)
            }

            /// Retrieves an EntityRef that exposes read-only operations for the Entity
            /// identified by the given LocalEntity for the specified user.
            ///
            /// Returns `None` if:
            /// - The user does not exist
            /// - The LocalEntity doesn't exist for that user
            /// - The entity does not exist in the world
            pub fn local_entity<W: WorldRefType<E>>(
                &self,
                world: W,
                user_key: &UserKey,
                local_entity: &LocalEntity,
            ) -> Option<EntityRef<'_, E, W>> {
                self.world_server.local_entity(world, user_key, local_entity)
            }

            /// Retrieves an EntityMut that exposes read and write operations for the Entity
            /// identified by the given LocalEntity for the specified user.
            ///
            /// Returns `None` if:
            /// - The user does not exist
            /// - The LocalEntity doesn't exist for that user
            /// - The entity does not exist in the world
            pub fn local_entity_mut<W: WorldMutType<E>>(
                &mut self,
                world: W,
                user_key: &UserKey,
                local_entity: &LocalEntity,
            ) -> Option<EntityMut<'_, E, W>> {
                self.world_server.local_entity_mut(world, user_key, local_entity)
            }
        }
    }
}

#[cfg(feature = "test_utils")]
impl<E: Copy + Eq + Hash + Send + Sync> Server<E> {
    pub fn set_global_entity_counter_for_test(&mut self, value: u64) {
        self.world_server.set_global_entity_counter_for_test(value);
    }

    pub fn diff_handler_global_count(&self) -> usize {
        self.world_server.diff_handler_global_count()
    }

    pub fn diff_handler_global_count_by_kind(
        &self,
    ) -> std::collections::HashMap<naia_shared::ComponentKind, usize> {
        self.world_server.diff_handler_global_count_by_kind()
    }

    pub fn diff_handler_user_counts(&self) -> std::collections::HashMap<UserKey, usize> {
        self.world_server.diff_handler_user_counts()
    }

    pub fn scope_change_queue_len(&self) -> usize {
        self.world_server.scope_change_queue_len()
    }

    pub fn total_dirty_update_count(&self) -> usize {
        self.world_server.total_dirty_update_count()
    }

    pub fn inject_tick_buffer_message<C: Channel, M: Message>(
        &mut self,
        user_key: &UserKey,
        host_tick: &Tick,
        message_tick: &Tick,
        message: &M,
    ) -> bool {
        self.world_server
            .inject_tick_buffer_message::<C, M>(user_key, host_tick, message_tick, message)
    }
}
