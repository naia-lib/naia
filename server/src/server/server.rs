use std::{hash::Hash, net::SocketAddr, panic, time::Duration};

use naia_shared::{
    AuthorityError, Channel, ComponentKind, EntityAndGlobalEntityConverter, EntityAuthStatus,
    EntityDoesNotExistError, GlobalEntity, Instant, Message, Protocol, Replicate, Request,
    Response, ResponseReceiveKey, ResponseSendKey, SocketConfig, Tick, WorldMutType, WorldRefType,
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

/// A server that uses either UDP or WebRTC communication to send/receive
/// messages to/from connected clients, and syncs registered entities to
/// clients to whom they are in-scope
pub struct Server<E: Copy + Eq + Hash + Send + Sync> {
    main_server: MainServer,
    outstanding_main_events: MainEvents,
    world_server: WorldServer<E>,
    to_world_sender_opt: Option<Box<dyn PacketSender>>,
}

impl<E: Copy + Eq + Hash + Send + Sync> Server<E> {
    /// Create a new Server
    pub fn new<P: Into<Protocol>>(server_config: ServerConfig, protocol: P) -> Self {
        let protocol: Protocol = protocol.into();

        Self {
            main_server: MainServer::new(server_config.clone(), protocol.clone()),
            outstanding_main_events: MainEvents::default(),
            world_server: WorldServer::new(server_config, protocol),
            to_world_sender_opt: None,
        }
    }

    /// Listen at the given addresses
    pub fn listen<S: Into<Box<dyn Socket>>>(&mut self, socket: S) {
        self.main_server.listen(socket);

        // load world io
        let world_io_sender = self.main_server.sender_cloned();
        let (to_world_sender, world_io_receiver) = PacketChannel::unbounded();
        self.to_world_sender_opt = Some(to_world_sender);
        self.world_server
            .io_load(world_io_sender, world_io_receiver);
    }

    /// Returns whether or not the Server has initialized correctly and is
    /// listening for Clients
    pub fn is_listening(&self) -> bool {
        self.main_server.is_listening()
    }

    /// Returns socket config
    pub fn socket_config(&self) -> &SocketConfig {
        self.main_server.socket_config()
    }

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

    pub fn process_all_packets<W: WorldMutType<E>>(&mut self, world: W, now: &Instant) {
        self.world_server.process_all_packets(world, &now);
    }

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

    pub fn take_tick_events(&mut self, now: &Instant) -> TickEvents {
        self.world_server.take_tick_events(&now)
    }

    // Connections

    /// Accepts an incoming Client User, allowing them to establish a connection
    /// with the Server
    pub fn accept_connection(&mut self, user_key: &UserKey) {
        self.main_server.accept_connection(user_key);
    }

    /// Rejects an incoming Client User, terminating their attempt to establish
    /// a connection with the Server
    pub fn reject_connection(&mut self, user_key: &UserKey) {
        self.main_server.reject_connection(user_key);
    }

    // Messages

    /// Queues up an Message to be sent to the Client associated with a given
    /// UserKey
    pub fn send_message<C: Channel, M: Message>(&mut self, user_key: &UserKey, message: &M) {
        self.world_server.send_message::<C, M>(user_key, message);
    }

    /// Sends a message to all connected users using a given channel
    pub fn broadcast_message<C: Channel, M: Message>(&mut self, message: &M) {
        self.world_server.broadcast_message::<C, M>(message);
    }

    pub fn send_request<C: Channel, Q: Request>(
        &mut self,
        user_key: &UserKey,
        request: &Q,
    ) -> Result<ResponseReceiveKey<Q::Response>, NaiaServerError> {
        self.world_server.send_request::<C, Q>(user_key, request)
    }

    /// Sends a Response for a given Request. Returns whether or not was successful.
    pub fn send_response<S: Response>(
        &mut self,
        response_key: &ResponseSendKey<S>,
        response: &S,
    ) -> bool {
        self.world_server.send_response(response_key, response)
    }

    pub fn receive_response<S: Response>(
        &mut self,
        response_key: &ResponseReceiveKey<S>,
    ) -> Option<(UserKey, S)> {
        self.world_server.receive_response(response_key)
    }
    //

    pub fn receive_tick_buffer_messages(&mut self, tick: &Tick) -> TickBufferMessages {
        self.world_server.receive_tick_buffer_messages(tick)
    }

    // Updates

    /// Used to evaluate whether, given a User & Entity that are in the
    /// same Room, said Entity should be in scope for the given User.
    ///
    /// While Rooms allow for a very simple scope to which an Entity can belong,
    /// this provides complete customization for advanced scopes.
    ///
    /// Return a collection of Entity Scope Sets, being a unique combination of
    /// a related Room, User, and Entity, used to determine which Entities to
    /// replicate to which Users
    pub fn scope_checks(&self) -> Vec<(RoomKey, UserKey, E)> {
        self.world_server.scope_checks()
    }

    /// Sends all update messages to all Clients. If you don't call this
    /// method, the Server will never communicate with it's connected
    /// Clients
    pub fn send_all_packets<W: WorldRefType<E>>(&mut self, world: W) {
        self.world_server.send_all_packets(world);
    }

    // Entities

    /// Creates a new Entity and returns an EntityMut which can be used for
    /// further operations on the Entity
    pub fn spawn_entity<W: WorldMutType<E>>(&'_ mut self, world: W) -> EntityMut<'_, E, W> {
        self.world_server.spawn_entity(world)
    }

    /// This is used only for Hecs/Bevy adapter crates, do not use otherwise!
    pub fn enable_entity_replication(&mut self, entity: &E) {
        self.world_server.enable_entity_replication(entity);
    }

    /// This is used only for Hecs/Bevy adapter crates, do not use otherwise!
    pub fn disable_entity_replication(&mut self, world_entity: &E) {
        self.world_server.disable_entity_replication(world_entity);
    }

    pub fn pause_entity_replication(&mut self, world_entity: &E) {
        self.world_server.pause_entity_replication(world_entity);
    }

    pub fn resume_entity_replication(&mut self, world_entity: &E) {
        self.world_server.resume_entity_replication(world_entity);
    }

    /// This is used only for Hecs/Bevy adapter crates, do not use otherwise!
    pub fn entity_replication_config(&self, world_entity: &E) -> Option<ReplicationConfig> {
        self.world_server.entity_replication_config(world_entity)
    }

    /// This is used only for Hecs/Bevy adapter crates, do not use otherwise!
    pub fn entity_take_authority(&mut self, world_entity: &E) -> Result<(), AuthorityError> {
        self.world_server.entity_take_authority(world_entity)
    }

    pub fn configure_entity_replication<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        world_entity: &E,
        config: ReplicationConfig,
    ) {
        self.world_server
            .configure_entity_replication(world, world_entity, config);
    }

    /// This is used only for Hecs/Bevy adapter crates, do not use otherwise!
    pub fn entity_authority_status(&self, world_entity: &E) -> Option<EntityAuthStatus> {
        self.world_server.entity_authority_status(world_entity)
    }

    /// This is used only for Hecs/Bevy adapter crates, do not use otherwise!
    pub fn entity_release_authority(
        &mut self,
        origin_user: Option<&UserKey>,
        world_entity: &E,
    ) -> Result<(), AuthorityError> {
        self.world_server
            .entity_release_authority(origin_user, world_entity)
    }

    /// Enable delegation for a server-owned entity
    ///
    /// This enables delegation for the given entity, allowing authority to be
    /// requested/released. The entity must be server-owned and Public.
    /// Returns true if delegation was enabled, false otherwise.
    pub fn enable_delegation<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        world_entity: &E,
    ) -> bool {
        self.world_server.enable_delegation(world, world_entity)
    }

    /// Retrieves an EntityRef that exposes read-only operations for the
    /// Entity.
    /// Panics if the Entity does not exist.
    pub fn entity<W: WorldRefType<E>>(&'_ self, world: W, entity: &E) -> EntityRef<'_, E, W> {
        self.world_server.entity(world, entity)
    }

    /// Retrieves an EntityMut that exposes read and write operations for the
    /// Entity.
    /// Panics if the Entity does not exist.
    pub fn entity_mut<W: WorldMutType<E>>(
        &'_ mut self,
        world: W,
        entity: &E,
    ) -> EntityMut<'_, E, W> {
        self.world_server.entity_mut(world, entity)
    }

    /// Gets a Vec of all Entities in the given World
    pub fn entities<W: WorldRefType<E>>(&self, world: W) -> Vec<E> {
        self.world_server.entities(world)
    }

    // This intended to be used by adapter crates, do not use!
    pub fn entity_owner(&self, world_entity: &E) -> EntityOwner {
        self.world_server.entity_owner(world_entity)
    }

    // Users

    /// Returns whether or not a User exists for the given RoomKey
    pub fn user_exists(&self, user_key: &UserKey) -> bool {
        self.main_server.user_exists(user_key)
    }

    /// Retrieves an UserRef that exposes read-only operations for the User
    /// associated with the given UserKey.
    /// Panics if the user does not exist.
    pub fn user(&'_ self, user_key: &UserKey) -> UserRef<'_, E> {
        if self.user_exists(user_key) {
            return UserRef::new(&self.world_server, user_key);
        }
        panic!("No User exists for given Key!");
    }

    /// Retrieves an UserMut that exposes read and write operations for the User
    /// associated with the given UserKey.
    /// Returns None if the user does not exist.
    pub fn user_mut(&'_ mut self, user_key: &UserKey) -> UserMut<'_, E> {
        if self.user_exists(user_key) {
            return UserMut::new(&mut self.world_server, user_key);
        }
        panic!("No User exists for given Key!");
    }

    /// Return a list of all currently connected Users' keys
    pub fn user_keys(&self) -> Vec<UserKey> {
        self.main_server.user_keys()
    }

    /// Get the number of Users currently connected
    pub fn users_count(&self) -> usize {
        self.main_server.users_count()
    }

    /// Returns a UserScopeRef, which is used to query whether a given user has
    pub fn user_scope(&'_ self, user_key: &UserKey) -> UserScopeRef<'_, E> {
        self.world_server.user_scope(user_key)
    }

    /// Returns a UserScopeMut, which is used to include/exclude Entities for a
    /// given User
    pub fn user_scope_mut(&'_ mut self, user_key: &UserKey) -> UserScopeMut<'_, E> {
        self.world_server.user_scope_mut(user_key)
    }

    // Rooms

    /// Creates a new Room on the Server and returns a corresponding RoomMut,
    /// which can be used to add users/entities to the room or retrieve its
    /// key
    pub fn make_room(&'_ mut self) -> RoomMut<'_, E> {
        self.world_server.make_room()
    }

    /// Returns whether or not a Room exists for the given RoomKey
    pub fn room_exists(&self, room_key: &RoomKey) -> bool {
        self.world_server.room_exists(room_key)
    }

    /// Retrieves an RoomMut that exposes read and write operations for the
    /// Room associated with the given RoomKey.
    /// Panics if the room does not exist.
    pub fn room(&'_ self, room_key: &RoomKey) -> RoomRef<'_, E> {
        self.world_server.room(room_key)
    }

    /// Retrieves an RoomMut that exposes read and write operations for the
    /// Room associated with the given RoomKey.
    /// Panics if the room does not exist.
    pub fn room_mut(&'_ mut self, room_key: &RoomKey) -> RoomMut<'_, E> {
        self.world_server.room_mut(room_key)
    }

    /// Return a list of all the Server's Rooms' keys
    pub fn room_keys(&self) -> Vec<RoomKey> {
        self.world_server.room_keys()
    }

    /// Get a count of how many Rooms currently exist
    pub fn rooms_count(&self) -> usize {
        self.world_server.rooms_count()
    }

    // Ticks

    /// Gets the current tick of the Server
    pub fn current_tick(&self) -> Tick {
        self.world_server.current_tick()
    }

    /// Gets the current average tick duration of the Server
    pub fn average_tick_duration(&self) -> Duration {
        self.world_server.average_tick_duration()
    }

    // Bandwidth monitoring
    pub fn outgoing_bandwidth_total(&mut self) -> f32 {
        self.world_server.outgoing_bandwidth_total()
    }

    pub fn incoming_bandwidth_total(&mut self) -> f32 {
        self.world_server.incoming_bandwidth_total()
    }

    pub fn outgoing_bandwidth_to_client(&mut self, address: &SocketAddr) -> f32 {
        self.world_server.outgoing_bandwidth_to_client(address)
    }

    pub fn incoming_bandwidth_from_client(&mut self, address: &SocketAddr) -> f32 {
        self.world_server.incoming_bandwidth_from_client(address)
    }

    // Ping
    /// Gets the average Round Trip Time measured to the given User's Client
    pub fn rtt(&self, user_key: &UserKey) -> Option<f32> {
        self.world_server.rtt(user_key)
    }

    /// Gets the average Jitter measured in connection to the given User's
    /// Client
    pub fn jitter(&self, user_key: &UserKey) -> Option<f32> {
        self.world_server.jitter(user_key)
    }

    // This intended to be used by adapter crates, do not use this as it will not update the world
    pub fn despawn_entity_worldless(&mut self, world_entity: &E) {
        self.world_server.despawn_entity_worldless(world_entity);
    }

    // This intended to be used by adapter crates, do not use this as it will not update the world
    pub fn insert_component_worldless(&mut self, world_entity: &E, component: &mut dyn Replicate) {
        self.world_server
            .insert_component_worldless(world_entity, component);
    }

    // This intended to be used by adapter crates, do not use this as it will not update the world
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
