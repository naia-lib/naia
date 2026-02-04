use std::{
    any::Any,
    collections::{hash_set::Iter, HashMap, HashSet},
    hash::Hash,
    net::SocketAddr,
    panic,
    time::Duration,
};

use log::{info, warn};

use naia_shared::{
    AuthorityError, handshake::HandshakeHeader, BigMap, BitReader, BitWriter, Channel, ChannelKind, ChannelKinds,
    ComponentKind, ComponentKinds, EntityAndGlobalEntityConverter, EntityAuthStatus,
    EntityDoesNotExistError, EntityEvent, GlobalEntity, GlobalEntityMap, GlobalEntitySpawner,
    GlobalRequestId, GlobalResponseId, GlobalWorldManagerType, HostType, Instant, Message,
    MessageContainer, MessageKinds, PacketType, Protocol, Replicate, ReplicatedComponent, Request,
    Response, ResponseReceiveKey, ResponseSendKey, Serde, SerdeErr, SharedGlobalWorldManager,
    StandardHeader, Tick, Timer, WorldMutType, WorldRefType,
};

use crate::{
    connection::{connection::Connection, io::Io, tick_buffer_messages::TickBufferMessages},
    events::{world_events::WorldEvents, TickEvents},
    handshake::HandshakeManager,
    request::{GlobalRequestManager, GlobalResponseManager},
    room::Room,
    time_manager::TimeManager,
    transport::{PacketReceiver, PacketSender},
    world::{
        entity_mut::EntityMut, entity_owner::EntityOwner, entity_ref::EntityRef,
        entity_room_map::EntityRoomMap, entity_scope_map::EntityScopeMap,
        global_world_manager::GlobalWorldManager, server_auth_handler::AuthOwner,
    },
    NaiaServerError, ReplicationConfig, RoomKey, RoomMut, RoomRef, ServerConfig, UserKey, UserMut,
    UserRef, UserScopeMut, UserScopeRef, WorldUser,
};

cfg_if! {
    if #[cfg(feature = "e2e_debug")] {
        use std::sync::atomic::{AtomicUsize, Ordering};
    }
}

#[cfg(feature = "e2e_debug")]
pub static SERVER_RX_FRAMES: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_TX_FRAMES: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_SPAWN_APPLIED: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_SEND_ALL_PACKETS_CALLS: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_OUTGOING_CMDS_DRAINED_TOTAL: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_AUTH_GRANTED_EMITTED: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_ROOM_MOVE_CALLED: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_SCOPE_DIFF_ENQUEUED: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_SET_AUTH_ENQUEUED: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_WORLD_MSGS_DRAINED: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_WROTE_SET_AUTH: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_WORLD_PKTS_SENT: AtomicUsize = AtomicUsize::new(0);

/// A server that uses either UDP or WebRTC communication to send/receive
/// messages to/from connected clients, and syncs registered entities to
/// clients to whom they are in-scope
pub struct WorldServer<E: Copy + Eq + Hash + Send + Sync> {
    server_config: ServerConfig,

    // Protocol
    channel_kinds: ChannelKinds,
    message_kinds: MessageKinds,
    component_kinds: ComponentKinds,
    client_authoritative_entities: bool,
    io: Io,
    // cont
    heartbeat_timer: Timer,
    ping_timer: Timer,
    timeout_timer: Timer,
    // Users
    users: HashMap<UserKey, WorldUser>,
    disconnected_users: HashMap<SocketAddr, UserKey>,
    user_connections: HashMap<SocketAddr, Connection>,
    // Rooms
    rooms: BigMap<RoomKey, Room>,
    // Entities
    entity_room_map: EntityRoomMap,
    entity_scope_map: EntityScopeMap,
    global_world_manager: GlobalWorldManager,
    global_entity_map: GlobalEntityMap<E>,
    // Events
    addrs_with_new_packets: HashSet<SocketAddr>,
    outstanding_disconnects: Vec<UserKey>,
    incoming_world_events: WorldEvents<E>,
    incoming_tick_events: TickEvents,
    // Requests/Responses
    global_request_manager: GlobalRequestManager,
    global_response_manager: GlobalResponseManager,
    // Ticks
    time_manager: TimeManager,
    // Deferred auth grants (one-tick delay to ensure entity registration)
    pending_auth_grants: Vec<(UserKey, GlobalEntity, EntityAuthStatus)>,
}

impl<E: Copy + Eq + Hash + Send + Sync> WorldServer<E> {
    /// Create a new WorldServer
    pub fn new<P: Into<Protocol>>(server_config: ServerConfig, protocol: P) -> Self {
        let protocol: Protocol = protocol.into();

        let Protocol {
            channel_kinds,
            message_kinds,
            component_kinds,
            tick_interval,
            compression,
            client_authoritative_entities,
            ..
        } = protocol;

        let heartbeat_timer = Timer::new(server_config.connection.heartbeat_interval);
        let ping_timer = Timer::new(server_config.ping.ping_interval);
        let timeout_timer = Timer::new(server_config.connection.disconnection_timeout_duration);

        let io = Io::new(
            &server_config.connection.bandwidth_measure_duration,
            &compression,
        );

        let time_manager = TimeManager::new(tick_interval);

        // Print protocol ID for SetAuthority at startup

        Self {
            // Config
            server_config,
            channel_kinds,
            message_kinds,
            component_kinds,
            client_authoritative_entities,
            io,
            heartbeat_timer,
            ping_timer,
            timeout_timer,
            // Users
            users: HashMap::new(),
            user_connections: HashMap::new(),
            disconnected_users: HashMap::new(),
            // Rooms
            rooms: BigMap::new(),
            // Entities
            entity_room_map: EntityRoomMap::new(),
            entity_scope_map: EntityScopeMap::new(),
            global_world_manager: GlobalWorldManager::new(),
            global_entity_map: GlobalEntityMap::new(),
            // Events
            addrs_with_new_packets: HashSet::new(),
            outstanding_disconnects: Vec::new(),
            incoming_world_events: WorldEvents::new(),
            incoming_tick_events: TickEvents::new(),
            // Requests/Responses
            global_request_manager: GlobalRequestManager::new(),
            global_response_manager: GlobalResponseManager::new(),
            time_manager,
            // Deferred auth grants
            pending_auth_grants: Vec::new(),
        }
    }

    /// Returns whether or not the Server has initialized correctly and is
    /// listening for Clients
    pub fn is_listening(&self) -> bool {
        self.io.is_loaded()
    }

    pub(crate) fn entity_converter(&self) -> &dyn EntityAndGlobalEntityConverter<E> {
        &self.global_entity_map
    }

    pub fn io_load(&mut self, sender: Box<dyn PacketSender>, receiver: Box<dyn PacketReceiver>) {
        self.io.load(sender, receiver);
    }

    pub fn receive_user(&mut self, user_key: UserKey, user_addr: SocketAddr) {
        self.users.insert(user_key, WorldUser::new(user_addr));
        self.disconnected_users.insert(user_addr, user_key);
    }

    fn finalize_connection(&mut self, user_key: &UserKey, user_address: &SocketAddr) {
        if !self.users.contains_key(user_key) {
            warn!("unknown user is finalizing connection...");
            return;
        };

        use std::collections::hash_map::Entry;
        let new_connection = Connection::new(
            &self.server_config.connection,
            &self.server_config.ping,
            user_address,
            &user_key,
            &self.channel_kinds,
            &self.global_world_manager,
        );

        match self.user_connections.entry(*user_address) {
            Entry::Vacant(v) => {
                v.insert(new_connection);
            }
            Entry::Occupied(mut o) => {
                o.insert(new_connection);
            }
        }

        if self.io.bandwidth_monitor_enabled() {
            self.io.register_client(user_address);
        }

        self.incoming_world_events.push_connection(&user_key);
    }

    /// Maintain connection with a client and read all incoming packet data
    pub fn receive_all_packets(&mut self) {
        // Tick bandwidth monitors to clear expired packets
        self.io.tick_bandwidth_monitors();

        self.handle_disconnects();
        self.handle_pings();
        self.handle_heartbeats();
        self.handle_empty_acks();

        let mut received_addresses = HashSet::new();

        // receive socket events
        loop {
            match self.io.recv_reader() {
                Ok(Some((address, owned_reader))) => {
                    // receive packet
                    let mut reader = owned_reader.borrow();

                    // read header
                    let Ok(header) = StandardHeader::de(&mut reader) else {
                        // Received a malformed packet
                        // TODO: increase suspicion against packet sender
                        continue;
                    };

                    received_addresses.insert(address);

                    match header.packet_type {
                        PacketType::Data => {
                            self.addrs_with_new_packets.insert(address);

                            if self
                                .read_data_packet(&address, &header, &mut reader)
                                .is_err()
                            {
                                warn!("Server Error: cannot read malformed packet");
                                continue;
                            }
                        }
                        PacketType::Heartbeat => {
                            if let Some(connection) = self.user_connections.get_mut(&address) {
                                connection.process_incoming_header(&header);
                            }

                            continue;
                        }
                        PacketType::Ping => {
                            let response = self.time_manager.process_ping(&mut reader).unwrap();
                            // send packet
                            if self.io.send_packet(&address, response.to_packet()).is_err() {
                                // TODO: pass this on and handle above
                                warn!("Server Error: Cannot send pong packet to {}", address);
                                continue;
                            };

                            if let Some(connection) = self.user_connections.get_mut(&address) {
                                connection.process_incoming_header(&header);
                            }

                            continue;
                        }
                        PacketType::Pong => {
                            if let Some(connection) = self.user_connections.get_mut(&address) {
                                connection.process_incoming_header(&header);
                                connection
                                    .ping_manager
                                    .process_pong(&self.time_manager, &mut reader);
                            }

                            continue;
                        }
                        PacketType::Handshake => {
                            let handshake_header_result = HandshakeHeader::de(&mut reader);
                            let Ok(HandshakeHeader::ClientConnectRequest) = handshake_header_result
                            else {
                                warn!(
                                    "Server Error: received invalid handshake packet: {:?}",
                                    handshake_header_result
                                );
                                continue;
                            };
                            let has_connection = self.user_connections.contains_key(&address);
                            if !has_connection {
                                let Some(user_key) = self.disconnected_users.remove(&address)
                                else {
                                    warn!("Server Error: received handshake packet from unknown address: {:?}", address);
                                    continue;
                                };
                                self.finalize_connection(&user_key, &address);
                            }

                            // Send Connect Response
                            let packet = HandshakeManager::write_connect_response().to_packet();
                            if self.io.send_packet(&address, packet).is_err() {
                                warn!(
                                    "Server Error: Cannot send handshake response to {}",
                                    address
                                );
                                continue;
                            }

                            continue;
                        }
                    }
                }
                Ok(None) => {
                    // No more packets, break loop
                    break;
                }
                Err(error) => {
                    self.incoming_world_events
                        .push_error(NaiaServerError::Wrapped(Box::new(error)));
                }
            }
        }

        for address in received_addresses {
            if let Some(connection) = self.user_connections.get_mut(&address) {
                connection.process_received_commands();
            }
        }
    }

    pub fn process_all_packets<W: WorldMutType<E>>(&mut self, mut world: W, now: &Instant) {
        self.process_disconnects(&mut world);

        let addresses = std::mem::take(&mut self.addrs_with_new_packets);
        for address in addresses {
            self.process_packets(&address, &mut world, now);
        }
    }

    pub fn take_world_events(&mut self) -> WorldEvents<E> {
        std::mem::replace(&mut self.incoming_world_events, WorldEvents::<E>::new())
    }

    pub fn take_tick_events(&mut self, now: &Instant) -> TickEvents {
        // tick event
        if self.time_manager.recv_server_tick(&now) {
            self.incoming_tick_events
                .push_tick(self.time_manager.current_tick());
        }
        std::mem::replace(&mut self.incoming_tick_events, TickEvents::new())
    }

    // Messages

    /// Queues up an Message to be sent to the Client associated with a given
    /// UserKey
    pub fn send_message<C: Channel, M: Message>(&mut self, user_key: &UserKey, message: &M) {
        let cloned_message = M::clone_box(message);
        self.send_message_inner(user_key, &ChannelKind::of::<C>(), cloned_message);
    }

    /// Queues up an Message to be sent to the Client associated with a given
    /// UserKey
    fn send_message_inner(
        &mut self,
        user_key: &UserKey,
        channel_kind: &ChannelKind,
        message_box: Box<dyn Message>,
    ) {
        let channel_settings = self.channel_kinds.channel(channel_kind);

        if !channel_settings.can_send_to_client() {
            panic!("Cannot send message to Client on this Channel");
        }

        let Some(user) = self.users.get(user_key) else {
            warn!("user: {:?} does not exist", user_key);
            return;
        };
        let Some(connection) = self.user_connections.get_mut(&user.address()) else {
            warn!("currently not connected to user: {:?}", user_key);
            return;
        };
        let mut converter = connection
            .base
            .world_manager
            .entity_converter_mut(&self.global_world_manager);
        let message = MessageContainer::from_write(message_box, &mut converter);
        connection.base.message_manager.send_message(
            &self.message_kinds,
            &mut converter,
            channel_kind,
            message,
        );
    }

    /// Sends a message to all connected users using a given channel
    pub fn broadcast_message<C: Channel, M: Message>(&mut self, message: &M) {
        let cloned_message = M::clone_box(message);
        self.broadcast_message_inner(&ChannelKind::of::<C>(), cloned_message);
    }

    fn broadcast_message_inner(
        &mut self,
        channel_kind: &ChannelKind,
        message_box: Box<dyn Message>,
    ) {
        self.user_keys().iter().for_each(|user_key| {
            self.send_message_inner(user_key, channel_kind, message_box.clone())
        })
    }

    //
    pub fn send_request<C: Channel, Q: Request>(
        &mut self,
        user_key: &UserKey,
        request: &Q,
    ) -> Result<ResponseReceiveKey<Q::Response>, NaiaServerError> {
        let cloned_request = Q::clone_box(request);
        let id = self.send_request_inner(user_key, &ChannelKind::of::<C>(), cloned_request)?;
        Ok(ResponseReceiveKey::new(id))
    }

    fn send_request_inner(
        &mut self,
        user_key: &UserKey,
        channel_kind: &ChannelKind,
        request_box: Box<dyn Message>,
    ) -> Result<GlobalRequestId, NaiaServerError> {
        let channel_settings = self.channel_kinds.channel(&channel_kind);

        if !channel_settings.can_request_and_respond() {
            panic!("Requests can only be sent over Bidirectional, Reliable Channels");
        }

        let request_id = self.global_request_manager.create_request_id(user_key);

        let Some(user) = self.users.get(user_key) else {
            warn!("user does not exist");
            return Err(NaiaServerError::Message("user does not exist".to_string()));
        };
        let Some(connection) = self.user_connections.get_mut(&user.address()) else {
            warn!("currently not connected to user");
            return Err(NaiaServerError::Message(
                "currently not connected to user".to_string(),
            ));
        };
        let mut converter = connection
            .base
            .world_manager
            .entity_converter_mut(&self.global_world_manager);

        let message = MessageContainer::from_write(request_box, &mut converter);
        connection.base.message_manager.send_request(
            &self.message_kinds,
            &mut converter,
            channel_kind,
            request_id,
            message,
        );

        return Ok(request_id);
    }

    /// Sends a Response for a given Request. Returns whether or not was successful.
    pub fn send_response<S: Response>(
        &mut self,
        response_key: &ResponseSendKey<S>,
        response: &S,
    ) -> bool {
        let response_id = response_key.response_id();

        let cloned_response = S::clone_box(response);

        self.send_response_inner(&response_id, cloned_response)
    }

    // returns whether was successful
    fn send_response_inner(
        &mut self,
        response_id: &GlobalResponseId,
        response_box: Box<dyn Message>,
    ) -> bool {
        let Some((user_key, channel_kind, local_response_id)) = self
            .global_response_manager
            .destroy_response_id(&response_id)
        else {
            return false;
        };
        let Some(user) = self.users.get(&user_key) else {
            return false;
        };
        let Some(connection) = self.user_connections.get_mut(&user.address()) else {
            return false;
        };
        let mut converter = connection
            .base
            .world_manager
            .entity_converter_mut(&self.global_world_manager);
        let response = MessageContainer::from_write(response_box, &mut converter);
        connection.base.message_manager.send_response(
            &self.message_kinds,
            &mut converter,
            &channel_kind,
            local_response_id,
            response,
        );
        return true;
    }

    pub fn receive_response<S: Response>(
        &mut self,
        response_key: &ResponseReceiveKey<S>,
    ) -> Option<(UserKey, S)> {
        let request_id = response_key.request_id();
        let Some((user_key, container)) =
            self.global_request_manager.destroy_request_id(&request_id)
        else {
            return None;
        };
        let response: S = Box::<dyn Any + 'static>::downcast::<S>(container.to_boxed_any())
            .ok()
            .map(|boxed_s| *boxed_s)
            .unwrap();
        return Some((user_key, response));
    }
    //

    pub fn receive_tick_buffer_messages(&mut self, tick: &Tick) -> TickBufferMessages {
        let mut tick_buffer_messages = TickBufferMessages::new();
        for (_user_address, connection) in self.user_connections.iter_mut() {
            // receive messages from anyone
            connection.tick_buffer_messages(tick, &mut tick_buffer_messages);
        }
        tick_buffer_messages
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
        let mut list: Vec<(RoomKey, UserKey, E)> = Vec::new();

        // TODO: precache this, instead of generating a new list every call
        // likely this is called A LOT
        for (room_key, room) in self.rooms.iter() {
            for user_key in room.user_keys() {
                for global_entity in room.entities() {
                    if let Ok(entity) = self
                        .global_entity_map
                        .global_entity_to_entity(global_entity)
                    {
                        list.push((room_key, *user_key, entity));
                    }
                }
            }
        }

        list
    }

    /// Sends all update messages to all Clients. If you don't call this
    /// method, the Server will never communicate with it's connected
    /// Clients
    pub fn send_all_packets<W: WorldRefType<E>>(&mut self, world: W) {
        #[cfg(feature = "e2e_debug")]
        {
            SERVER_SEND_ALL_PACKETS_CALLS.fetch_add(1, Ordering::Relaxed);
        }
        let now = Instant::now();

        // update entity scopes
        self.update_entity_scopes(&world);

        // loop through all connections, send packet
        let mut user_addresses: Vec<SocketAddr> = self.user_connections.keys().copied().collect();
        // shuffle order of connections in order to avoid priority among users
        fastrand::shuffle(&mut user_addresses);

        for user_address in user_addresses {
            let connection = self.user_connections.get_mut(&user_address).unwrap();
            connection.send_packets(
                &self.channel_kinds,
                &self.message_kinds,
                &self.component_kinds,
                &now,
                &mut self.io,
                &world,
                &self.global_entity_map,
                &self.global_world_manager,
                &self.time_manager,
            );
        }

        // Flush deferred auth grants (one-tick delay ensures entity registration on client)
        let pending_grants = std::mem::take(&mut self.pending_auth_grants);
        for (owner_user_key, global_entity, _granted_status) in pending_grants {
            // Collect addresses first to avoid borrowing issues
            let user_addresses: Vec<SocketAddr> = self.user_connections.keys().copied().collect();
            // Send SetAuthority to all users in scope (canonical path)
            for address in user_addresses {
                let Some(conn) = self.user_connections.get_mut(&address) else {
                    continue;
                };
                if !conn.base.world_manager.has_global_entity(&global_entity) {
                    continue;
                }
                let user_key_for_conn = conn.user_key;
                let mut new_status: EntityAuthStatus = EntityAuthStatus::Denied;
                if owner_user_key == user_key_for_conn {
                    new_status = EntityAuthStatus::Granted;
                }
                // Use host_send_set_auth which handles both HostEntity and RemoteEntity
                conn.base.world_manager.host_send_set_auth(&global_entity, new_status);
                #[cfg(feature = "e2e_debug")]
                if new_status == EntityAuthStatus::Granted {
                    SERVER_SET_AUTH_ENQUEUED.fetch_add(1, Ordering::Relaxed);
                    SERVER_AUTH_GRANTED_EMITTED.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    // Entities

    /// Creates a new Entity and returns an EntityMut which can be used for
    /// further operations on the Entity
    pub fn spawn_entity<W: WorldMutType<E>>(&'_ mut self, mut world: W) -> EntityMut<'_, E, W> {
        let world_entity = world.spawn_entity();

        self.spawn_entity_inner(&world_entity);

        EntityMut::new(self, world, &world_entity)
    }

    /// Creates a new Entity with a specific id
    fn spawn_entity_inner(&mut self, world_entity: &E) {
        let global_entity = self.global_entity_map.spawn(*world_entity, None);
        self.global_world_manager
            .insert_entity_record(&global_entity, EntityOwner::Server);
    }

    /// This is used only for Hecs/Bevy adapter crates, do not use otherwise!
    pub fn enable_entity_replication(&mut self, entity: &E) {
        self.spawn_entity_inner(&entity);
    }

    /// This is used only for Hecs/Bevy adapter crates, do not use otherwise!
    pub fn disable_entity_replication(&mut self, world_entity: &E) {
        // Despawn from connections and inner tracking
        self.despawn_entity_worldless(world_entity);
    }

    pub fn pause_entity_replication(&mut self, world_entity: &E) {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();
        self.global_world_manager
            .pause_entity_replication(&global_entity);
    }

    pub fn resume_entity_replication(&mut self, world_entity: &E) {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();
        self.global_world_manager
            .resume_entity_replication(&global_entity);
    }

    #[cfg(feature = "test_utils")]
    pub fn set_global_entity_counter_for_test(&mut self, value: u64) {
        self.global_entity_map.set_global_entity_counter_for_test(value);
    }

    /// This is used only for Hecs/Bevy adapter crates, do not use otherwise!
    pub fn entity_replication_config(&self, world_entity: &E) -> Option<ReplicationConfig> {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();
        self.global_world_manager
            .entity_replication_config(&global_entity)
    }

    /// This is used only for Hecs/Bevy adapter crates, do not use otherwise!
    pub fn entity_take_authority(&mut self, world_entity: &E) -> Result<(), AuthorityError> {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();
        let result = self
            .global_world_manager
            .server_take_authority(&global_entity);

        if let Ok(previous_owner) = result {
            // When server takes authority, send Denied to clients whose state will change:
            // - If there was a client holder (Granted→Denied): send only to that client
            // - If no holder (Available→Denied): send to all clients in scope
            self.send_take_authority_messages(&global_entity, previous_owner);
            self.incoming_world_events.push_auth_reset(world_entity);
        }
        result.map(|_| ())
    }

    fn send_take_authority_messages(&mut self, global_entity: &GlobalEntity, previous_owner: AuthOwner) {
        // Server has taken authority - send appropriate messages based on previous state
        match previous_owner {
            AuthOwner::Client(prev_holder_key) => {
                // There was a client holder - only they need to transition (Granted→Denied)
                // Other clients were already Denied, no message needed
                if let Some(user) = self.users.get(&prev_holder_key) {
                    if let Some(connection) = self.user_connections.get_mut(&user.address()) {
                        if connection.base.world_manager.has_global_entity(global_entity) {
                            connection.base.world_manager.host_send_set_auth(global_entity, EntityAuthStatus::Denied);
                        }
                    }
                }
            }
            AuthOwner::None => {
                // No holder - all clients were Available, all need to transition to Denied
                for (_user_key, user) in self.users.iter() {
                    if let Some(connection) = self.user_connections.get_mut(&user.address()) {
                        if !connection.base.world_manager.has_global_entity(global_entity) {
                            continue;
                        }
                        connection.base.world_manager.host_send_set_auth(global_entity, EntityAuthStatus::Denied);
                    }
                }
            }
            AuthOwner::Server => {
                // Server already had authority - no change needed
            }
        }
    }

    fn send_reset_authority_messages(&mut self, global_entity: &GlobalEntity) {
        // authority was released from entity
        // for any users that have this entity in scope, send an `update_authority_status` message

        // TODO: we can make this more efficient in the future by caching which Entities
        // are in each User's scope
        for (_user_key, user) in self.users.iter() {
            if let Some(connection) = self.user_connections.get_mut(&user.address()) {
                // Check if entity exists on the client (as either HostEntity or RemoteEntity)
                // After migration, the entity is a RemoteEntity on the client, but the server
                // still sends from HostEntity perspective and the client's routing handles it
                if !connection
                    .base
                    .world_manager
                    .has_global_entity(global_entity)
                {
                    // entity is not mapped to this connection
                    continue;
                }

                // Send UpdateAuthority action through EntityActionEvent system
                // The server always sends from HostEntity perspective, and the client's
                // routing logic will handle converting it to the correct entity type
                connection
                    .base
                    .world_manager
                    .host_send_set_auth(global_entity, EntityAuthStatus::Available);
            }
        }
    }

    pub fn configure_entity_replication<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        world_entity: &E,
        config: ReplicationConfig,
    ) {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();
        if !self.global_world_manager.has_entity(&global_entity) {
            panic!("Entity is not yet replicating. Be sure to call `enable_replication` or `spawn_entity` on the Server, before configuring replication.");
        }
        let entity_owner = self
            .global_world_manager
            .entity_owner(&global_entity)
            .unwrap();
        let server_owned: bool = entity_owner.is_server();
        let client_owned: bool = entity_owner.is_client();
        let next_config = config;
        let prev_config = self
            .global_world_manager
            .entity_replication_config(&global_entity)
            .unwrap();
        if prev_config == config {
            // Already in the desired state, no-op
            return;
        }

        match prev_config {
            ReplicationConfig::Private => {
                if server_owned {
                    panic!("Server-owned entity should never be private");
                }
                match next_config {
                    ReplicationConfig::Private => {
                        panic!("Should not be able to happen");
                    }
                    ReplicationConfig::Public => {
                        // private -> public
                        self.publish_entity(world, &global_entity, world_entity, true);
                    }
                    ReplicationConfig::Delegated => {
                        // private -> delegated
                        // Per spec [entity-ownership-11], server CAN enable delegation on client-owned entities,
                        // which transfers ownership to server
                        self.publish_entity(world, &global_entity, world_entity, true);
                        self.entity_enable_delegation(world, &global_entity, world_entity, None);
                    }
                }
            }
            ReplicationConfig::Public => {
                match next_config {
                    ReplicationConfig::Private => {
                        // public -> private
                        if server_owned {
                            panic!("Cannot unpublish a Server-owned Entity (doing so would disable replication entirely, just use a local entity instead)");
                        }
                        self.unpublish_entity(world, &global_entity, world_entity, true);
                    }
                    ReplicationConfig::Public => {
                        panic!("Should not be able to happen");
                    }
                    ReplicationConfig::Delegated => {
                        // public -> delegated
                        // Per spec [entity-ownership-11], server CAN enable delegation on client-owned entities,
                        // which transfers ownership to server
                        self.entity_enable_delegation(world, &global_entity, world_entity, None);
                    }
                }
            }
            ReplicationConfig::Delegated => {
                if client_owned {
                    panic!("Client-owned entity should never be delegated");
                }
                match next_config {
                    ReplicationConfig::Private => {
                        // delegated -> private
                        if server_owned {
                            panic!("Cannot unpublish a Server-owned Entity (doing so would disable replication entirely, just use a local entity instead)");
                        }
                        self.entity_disable_delegation(world, &global_entity, world_entity);
                        self.unpublish_entity(world, &global_entity, world_entity, true);
                    }
                    ReplicationConfig::Public => {
                        // delegated -> public
                        self.entity_disable_delegation(world, &global_entity, world_entity);
                    }
                    ReplicationConfig::Delegated => {
                        panic!("Should not be able to happen");
                    }
                }
            }
        }
    }

    /// This is used only for Hecs/Bevy adapter crates, do not use otherwise!
    pub(crate) fn entity_give_authority(
        &mut self,
        origin_user: &UserKey,
        world_entity: &E,
    ) -> Result<(), AuthorityError> {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();
        
        let requester = AuthOwner::Client(*origin_user);
        let result = self
            .global_world_manager
            .client_request_authority(&global_entity, &requester);
        if result.is_err() {
            return result;
        }

        // entity authority was granted for origin user

        // for any users that have this entity in scope, send an `update_authority_status` message

        // TODO: we can make this more efficient in the future by caching which Entities
        // are in each User's scope
        for (user_key, user) in self.users.iter() {
            let Some(connection) = self.user_connections.get_mut(&user.address()) else {
                continue;
            };
            // Check if entity exists on the client (as either HostEntity or RemoteEntity)
            // After migration, the entity is a RemoteEntity on the client, but the server
            // still sends from HostEntity perspective and the client's routing handles it
            if !connection
                .base
                .world_manager
                .has_global_entity(&global_entity)
            {
                // entity is not mapped to this connection
                continue;
            }

            let mut new_status: EntityAuthStatus = EntityAuthStatus::Denied;
            if origin_user == user_key {
                new_status = EntityAuthStatus::Granted;
            }

            // Send UpdateAuthority action through EntityActionEvent system
            // The server always sends from HostEntity perspective, and the client's
            // routing logic will handle converting it to the correct entity type
            connection
                .base
                .world_manager
                .host_send_set_auth(&global_entity, new_status);
            #[cfg(feature = "e2e_debug")]
            if new_status == EntityAuthStatus::Granted {
                SERVER_SET_AUTH_ENQUEUED.fetch_add(1, Ordering::Relaxed);
                SERVER_AUTH_GRANTED_EMITTED.fetch_add(1, Ordering::Relaxed);
            }
        }

        // From a previous failure:
        // We send SetAuthority immediately above (where though?), so we don't need to push to auth_grants
        // which would cause duplicate SetAuthority sends (leading to invalid transitions like Denied->Denied)
        // Potentially there will be a future error here!
        // TODO: verify this with tests!

        // Push to events for external systems (e.g., Bevy adapter, test harness)
        // Events are separate from network messages - they're notifications for external consumers
        self.incoming_world_events.push_auth_grant(origin_user, &world_entity);
        
        Ok(())
    }

    fn entity_enable_delegation_response(
        &mut self,
        _user_key: &UserKey,
        _global_entity: &GlobalEntity,
    ) {
        // EnableDelegationResponse does NOT send SetAuthority messages.
        // Enabling delegation establishes the delegated-mode baseline as Available (AuthNone) for clients.
        // Any Denied/Granted status changes come ONLY from subsequent authority operations (request/give/take/release).
        // The client initializes local auth status to Available when processing EnableDelegation message,
        // so no SetAuthority message is needed here.
    }

    /// This is used only for Hecs/Bevy adapter crates, do not use otherwise!
    pub(crate) fn entity_authority_status(&self, world_entity: &E) -> Option<EntityAuthStatus> {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();
        self.global_world_manager
            .entity_authority_status(&global_entity)
    }

    /// This is used only for Hecs/Bevy adapter crates, do not use otherwise!
    pub fn entity_release_authority(
        &mut self,
        origin_user: Option<&UserKey>,
        world_entity: &E,
    ) -> Result<(), AuthorityError> {
        let releaser = AuthOwner::from_user_key(origin_user);
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();
        let result = self
            .global_world_manager
            .client_release_authority(&global_entity, &releaser);
        if result.is_ok() {
            self.send_reset_authority_messages(&global_entity);
        }
        result
    }

    /// Enable delegation for a server-owned entity
    ///
    /// This enables delegation for the given entity, allowing authority to be
    /// requested/released. The entity must be server-owned and Public.
    /// Returns true if delegation was enabled, false otherwise.
    pub(crate) fn enable_delegation<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        world_entity: &E,
    ) -> bool {
        let global_entity = match self
            .global_entity_map
            .entity_to_global_entity(world_entity)
        {
            Ok(ge) => ge,
            Err(_) => return false,
        };

        // Only enable delegation for server-owned entities
        let owner = self.entity_owner(world_entity);
        if !owner.is_server() {
            return false;
        }

        self.entity_enable_delegation(world, &global_entity, world_entity, None);
        true
    }

    /// Retrieves an EntityRef that exposes read-only operations for the
    /// Entity.
    /// Panics if the Entity does not exist.
    pub fn entity<W: WorldRefType<E>>(&'_ self, world: W, entity: &E) -> EntityRef<'_, E, W> {
        if world.has_entity(entity) {
            return EntityRef::new(self, world, entity);
        }
        panic!("No Entity exists for given Key!");
    }

    /// Retrieves an EntityMut that exposes read and write operations for the
    /// Entity.
    /// Panics if the Entity does not exist.
    pub fn entity_mut<W: WorldMutType<E>>(
        &'_ mut self,
        world: W,
        entity: &E,
    ) -> EntityMut<'_, E, W> {
        if world.has_entity(entity) {
            return EntityMut::new(self, world, entity);
        }
        panic!("No Entity exists for given Key!");
    }

    /// Gets a Vec of all Entities in the given World
    pub fn entities<W: WorldRefType<E>>(&self, world: W) -> Vec<E> {
        world.entities()
    }

    // This intended to be used by adapter crates
    pub(crate) fn entity_owner(&self, world_entity: &E) -> EntityOwner {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();
        if let Some(owner) = self.global_world_manager.entity_owner(&global_entity) {
            return owner;
        }
        return EntityOwner::Local;
    }

    // Users

    /// Returns whether or not a User exists for the given RoomKey
    pub fn user_exists(&self, user_key: &UserKey) -> bool {
        self.users.contains_key(user_key)
    }

    /// Retrieves an UserRef that exposes read-only operations for the User
    /// associated with the given UserKey.
    /// Panics if the user does not exist.
    pub fn user(&'_ self, user_key: &UserKey) -> UserRef<'_, E> {
        if self.users.contains_key(user_key) {
            return UserRef::new(self, user_key);
        }
        panic!("No User exists for given Key!");
    }

    /// Retrieves an UserMut that exposes read and write operations for the User
    /// associated with the given UserKey.
    /// Returns None if the user does not exist.
    pub fn user_mut(&'_ mut self, user_key: &UserKey) -> UserMut<'_, E> {
        if self.users.contains_key(user_key) {
            return UserMut::new(self, user_key);
        }
        panic!("No User exists for given Key!");
    }

    /// Return a list of all currently connected Users' keys
    pub fn user_keys(&self) -> Vec<UserKey> {
        let mut output = Vec::new();

        for (user_key, user) in self.users.iter() {
            if self.user_connections.contains_key(&user.address()) {
                output.push(*user_key);
            }
        }

        output
    }

    /// Get the number of Users currently connected
    pub fn users_count(&self) -> usize {
        self.users.len()
    }

    /// Returns a UserScopeRef, which is used to query whether a given user has
    pub fn user_scope(&'_ self, user_key: &UserKey) -> UserScopeRef<'_, E> {
        if self.users.contains_key(user_key) {
            return UserScopeRef::new(self, user_key);
        }
        panic!("No User exists for given Key!");
    }

    /// Returns a UserScopeMut, which is used to include/exclude Entities for a
    /// given User
    pub fn user_scope_mut(&'_ mut self, user_key: &UserKey) -> UserScopeMut<'_, E> {
        if self.users.contains_key(user_key) {
            return UserScopeMut::new(self, user_key);
        }
        panic!("No User exists for given Key!");
    }

    // Ticks

    /// Gets the current tick of the Server
    pub fn current_tick(&self) -> Tick {
        return self.time_manager.current_tick();
    }

    /// Gets the current average tick duration of the Server
    pub fn average_tick_duration(&self) -> Duration {
        self.time_manager.average_tick_duration()
    }

    // Rooms

    /// Creates a new Room on the Server and returns a corresponding RoomMut,
    /// which can be used to add users/entities to the room or retrieve its
    /// key
    pub fn make_room(&'_ mut self) -> RoomMut<'_, E> {
        let new_room = Room::new();
        let room_key = self.rooms.insert(new_room);
        RoomMut::new(self, &room_key)
    }

    /// Returns whether or not a Room exists for the given RoomKey
    pub fn room_exists(&self, room_key: &RoomKey) -> bool {
        self.rooms.contains_key(room_key)
    }

    /// Retrieves an RoomMut that exposes read and write operations for the
    /// Room associated with the given RoomKey.
    /// Panics if the room does not exist.
    pub fn room(&'_ self, room_key: &RoomKey) -> RoomRef<'_, E> {
        if self.rooms.contains_key(room_key) {
            return RoomRef::new(self, room_key);
        }
        panic!("No Room exists for given Key!");
    }

    /// Retrieves an RoomMut that exposes read and write operations for the
    /// Room associated with the given RoomKey.
    /// Panics if the room does not exist.
    pub fn room_mut(&'_ mut self, room_key: &RoomKey) -> RoomMut<'_, E> {
        if self.rooms.contains_key(room_key) {
            return RoomMut::new(self, room_key);
        }
        panic!("No Room exists for given Key!");
    }

    /// Return a list of all the Server's Rooms' keys
    pub fn room_keys(&self) -> Vec<RoomKey> {
        let mut output = Vec::new();

        for (key, _) in self.rooms.iter() {
            output.push(key);
        }

        output
    }

    /// Get a count of how many Rooms currently exist
    pub fn rooms_count(&self) -> usize {
        self.rooms.len()
    }

    // Bandwidth monitoring
    pub fn outgoing_bandwidth_total(&self) -> f32 {
        self.io.outgoing_bandwidth_total()
    }

    pub fn incoming_bandwidth_total(&self) -> f32 {
        self.io.incoming_bandwidth_total()
    }

    pub fn outgoing_bandwidth_to_client(&self, address: &SocketAddr) -> f32 {
        self.io.outgoing_bandwidth_to_client(address)
    }

    pub fn incoming_bandwidth_from_client(&self, address: &SocketAddr) -> f32 {
        self.io.incoming_bandwidth_from_client(address)
    }

    // Ping
    /// Gets the average Round Trip Time measured to the given User's Client
    pub fn rtt(&self, user_key: &UserKey) -> Option<f32> {
        if let Some(user) = self.users.get(user_key) {
            if let Some(connection) = self.user_connections.get(&user.address()) {
                return Some(connection.ping_manager.rtt_average);
            }
        }
        None
    }

    /// Gets the average Jitter measured in connection to the given User's
    /// Client
    pub fn jitter(&self, user_key: &UserKey) -> Option<f32> {
        if let Some(user) = self.users.get(user_key) {
            if let Some(connection) = self.user_connections.get(&user.address()) {
                return Some(connection.ping_manager.jitter_average);
            }
        }
        None
    }

    // Crate-Public methods

    //// Entities

    /// Despawns the Entity, if it exists.
    /// This will also remove all of the Entity’s Components.
    /// Panics if the Entity does not exist.
    pub(crate) fn despawn_entity<W: WorldMutType<E>>(&mut self, world: &mut W, world_entity: &E) {
        if !world.has_entity(&world_entity) {
            panic!("attempted to de-spawn nonexistent entity");
        }

        // Delete from world
        world.despawn_entity(&world_entity);

        self.despawn_entity_worldless(&world_entity);
    }

    // Used by adapter crates only!
    pub fn despawn_entity_worldless(&mut self, world_entity: &E) {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();
        if !self.global_world_manager.has_entity(&global_entity) {
            info!("attempting to despawn entity that does not exist, this can happen if a delegated entity is being despawned");
            return;
        }
        self.cleanup_entity_replication(&global_entity);
        self.global_world_manager
            .remove_entity_record(&global_entity);
        self.global_entity_map.despawn_by_global(&global_entity);
    }

    fn cleanup_entity_replication(&mut self, global_entity: &GlobalEntity) {
        self.despawn_entity_from_all_connections(global_entity);

        // Delete scope
        self.entity_scope_map.remove_entity(global_entity);

        // Delete room cache entry
        if let Some(room_keys) = self.entity_room_map.remove_from_all_rooms(global_entity) {
            for room_key in room_keys {
                if let Some(room) = self.rooms.get_mut(&room_key) {
                    room.remove_entity(global_entity, true);
                }
            }
        }

        // Remove from ECS Record
        self.global_world_manager
            .remove_entity_diff_handlers(global_entity);
    }

    fn despawn_entity_from_all_connections(&mut self, global_entity: &GlobalEntity) {
        // TODO: we can make this more efficient in the future by caching which Entities
        // are in each User's scope
        for (_, connection) in self.user_connections.iter_mut() {
            if !connection
                .base
                .world_manager
                .has_global_entity(global_entity)
            {
                continue;
            }
            // remove entity from user connection
            connection.base.world_manager.despawn_entity(global_entity);
        }
    }

    //// Entity Scopes

    /// Remove all entities from a User's scope
    pub(crate) fn user_scope_remove_user(&mut self, user_key: &UserKey) {
        self.entity_scope_map.remove_user(user_key);
    }

    pub(crate) fn user_scope_set_entity(
        &mut self,
        user_key: &UserKey,
        world_entity: &E,
        is_contained: bool,
    ) {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();

        // Per [entity-authority-12]: If the authority-holding client loses scope for E,
        // the server MUST release/reset authority for E.
        // Check if user is being removed from scope and is the authority holder
        if !is_contained
            && self
                .global_world_manager
                .user_is_authority_holder(user_key, &global_entity)
        {
            // Release authority - the user is losing scope while holding authority
            let releaser = AuthOwner::Client(*user_key);
            if self
                .global_world_manager
                .client_release_authority(&global_entity, &releaser)
                .is_ok()
            {
                // Notify other clients that authority is now Available
                self.send_reset_authority_messages(&global_entity);
            }
        }

        self.entity_scope_map
            .insert(*user_key, global_entity, is_contained);
    }

    pub(crate) fn user_scope_has_entity(&self, user_key: &UserKey, world_entity: &E) -> bool {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();

        // Check if entity has Private replication config
        let is_private = if let Some(config) = self.global_world_manager.entity_replication_config(&global_entity) {
            matches!(config, ReplicationConfig::Private)
        } else {
            false
        };

        // Owning client is always in-scope for client-owned entities
        let is_owner = if let Some(owner) = self.global_world_manager.entity_owner(&global_entity) {
            match owner {
                EntityOwner::Client(owner_key)
                | EntityOwner::ClientWaiting(owner_key)
                | EntityOwner::ClientPublic(owner_key) => {
                    owner_key == *user_key
                }
                _ => false
            }
        } else {
            false
        };

        // If owner, always in scope
        if is_owner {
            return true;
        }

        // Per [entity-publication]: Private entities MUST NOT be in-scope for non-owners
        if is_private {
            return false;
        }

        // Check explicit include/exclude
        if let Some(in_scope) = self.entity_scope_map.get(user_key, &global_entity) {
            return *in_scope;
        }
        // Default: in-scope if user and entity share a room
        let Some(user) = self.users.get(user_key) else {
            return false;
        };
        let Some(entity_rooms) = self.entity_room_map.entity_get_rooms(&global_entity) else {
            return false;
        };
        let user_rooms = user.room_keys();
        entity_rooms.intersection(user_rooms).next().is_some()
    }

    //// Components

    /// Adds a Component to an Entity
    pub(crate) fn insert_component<R: ReplicatedComponent, W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        world_entity: &E,
        mut component: R,
    ) {
        if !world.has_entity(world_entity) {
            panic!("attempted to add component to non-existent entity");
        }

        let component_kind = component.kind();

        if world.has_component_of_kind(world_entity, &component_kind) {
            // Entity already has this Component type yet, update Component

            let Some(mut component_mut) = world.component_mut::<R>(world_entity) else {
                panic!("Should never happen because we checked for this above");
            };
            component_mut.mirror(&component);
        } else {
            // Entity does not have this Component type yet, initialize Component
            self.insert_component_worldless(world_entity, &mut component);

            // actually insert component into world
            world.insert_component(world_entity, component);
        }
    }

    // This intended to be used by adapter crates, do not use this as it will not update the world
    pub fn insert_component_worldless(&mut self, world_entity: &E, component: &mut dyn Replicate) {
        let component_kind = component.kind();

        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();

        if self
            .global_world_manager
            .has_component_record(&global_entity, &component_kind)
        {
            warn!(
                "Attempted to add component `{:?}` to entity `{:?}` that already has it, this can happen if a delegated entity's auth is transferred to the Server before the Server Adapter has been able to process the newly inserted Component. Skipping this action.",
                component.name(), global_entity,
            );
            return;
        }

        self.insert_new_component_into_entity_scopes(&global_entity, &component_kind, None);

        // update in world manager
        self.global_world_manager.insert_component_record(
            // &self.component_kinds,
            &global_entity,
            &component_kind,
        );
        self.global_world_manager.insert_component_diff_handler(
            &self.component_kinds,
            &global_entity,
            component,
        );

        // if entity is delegated, convert over
        if self
            .global_world_manager
            .entity_is_delegated(&global_entity)
        {
            let accessor = self
                .global_world_manager
                .get_entity_auth_accessor(&global_entity);
            component.enable_delegation(&accessor, None)
        }
    }

    fn insert_new_component_into_entity_scopes(
        &mut self,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
        excluding_user_opt: Option<&UserKey>,
    ) {
        let excluding_addr_opt: Option<SocketAddr> = {
            if let Some(user_key) = excluding_user_opt {
                if let Some(user) = self.users.get(user_key) {
                    Some(user.address())
                } else {
                    None
                }
            } else {
                None
            }
        };
        // add component to connections already tracking entity
        for (addr, connection) in self.user_connections.iter_mut() {
            if let Some(exclude_addr) = excluding_addr_opt {
                if addr == &exclude_addr {
                    continue;
                }
            }

            // insert component into user's connection
            let has_entity = connection
                .base
                .world_manager
                .has_global_entity(global_entity);

            if !has_entity {
                // entity is not in scope for this connection
                continue;
            }
            connection
                .base
                .world_manager
                .insert_component(global_entity, &component_kind);
        }
    }

    /// Removes a Component from an Entity
    pub(crate) fn remove_component<R: ReplicatedComponent, W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        world_entity: &E,
    ) -> Option<R> {
        self.remove_component_worldless(world_entity, &ComponentKind::of::<R>());

        // remove from world
        world.remove_component::<R>(world_entity)
    }

    // This intended to be used by adapter crates, do not use this as it will not update the world
    pub fn remove_component_worldless(&mut self, world_entity: &E, component_kind: &ComponentKind) {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();
        self.remove_component_from_all_connections(&global_entity, component_kind);

        // cleanup all other loose ends
        self.global_world_manager
            .remove_component_record(&global_entity, &component_kind);
        self.global_world_manager
            .remove_component_diff_handler(&global_entity, &component_kind);
    }

    fn remove_component_from_all_connections(
        &mut self,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) {
        // TODO: should be able to make this more efficient by caching for every Entity
        // which scopes they are part of
        for (_, connection) in self.user_connections.iter_mut() {
            if !connection
                .base
                .world_manager
                .has_global_entity(global_entity)
            {
                // entity is not in scope for this connection
                continue;
            }
            // remove component from user connection
            connection
                .base
                .world_manager
                .remove_component(global_entity, &component_kind);
        }
    }

    //// Authority

    pub(crate) fn publish_entity<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: &GlobalEntity,
        world_entity: &E,
        server_origin: bool,
    ) -> bool {
        if server_origin {
            // send publish message to entity owner
            let entity_owner = self.global_world_manager.entity_owner(&global_entity);
            let Some(EntityOwner::Client(user_key)) = entity_owner else {
                panic!(
                    "Entity is not owned by a Client. Cannot publish entity. Owner is: {:?}",
                    entity_owner
                );
            };
            // Send PublishEntity action through EntityActionEvent system
            if let Some(user) = self.users.get(&user_key) {
                if let Some(connection) = self.user_connections.get_mut(&user.address()) {
                    connection
                        .base
                        .world_manager
                        .send_publish(HostType::Server, global_entity);
                }
            }
        }

        let result = self.global_world_manager.entity_publish(&global_entity);
        if result {
            world.entity_publish(
                &self.component_kinds,
                &self.global_entity_map,
                &self.global_world_manager,
                world_entity,
            );
        }
        return result;
    }

    pub(crate) fn unpublish_entity<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: &GlobalEntity,
        world_entity: &E,
        server_origin: bool,
    ) {
        if server_origin {
            // send publish message to entity owner
            let entity_owner = self.global_world_manager.entity_owner(&global_entity);
            let Some(EntityOwner::ClientPublic(user_key)) = entity_owner else {
                panic!("Entity is not owned by a Client or is Private. Cannot publish entity. Owner is: {:?}", entity_owner);
            };
            // Send UnpublishEntity action through EntityActionEvent system
            if let Some(user) = self.users.get(&user_key) {
                if let Some(connection) = self.user_connections.get_mut(&user.address()) {
                    connection
                        .base
                        .world_manager
                        .send_unpublish(HostType::Server, global_entity);
                }
            }
        }

        self.global_world_manager.entity_unpublish(&global_entity);
        world.entity_unpublish(world_entity);
        self.cleanup_entity_replication(&global_entity);
    }

    pub(crate) fn entity_enable_delegation<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: &GlobalEntity,
        world_entity: &E,
        client_origin: Option<UserKey>,
    ) {
        // TODO: check that entity is eligible for delegation?

        {
            // For any users that have this entity in scope,
            // Send an `enable_delegation` message

            // TODO: we can make this more efficient in the future by caching which Entities
            // are in each User's scope
            for (user_key, user) in self.users.iter() {
                if let Some(client_key) = &client_origin {
                    if user_key == client_key {
                        // skip sending to origin client, will be handled below
                        continue;
                    }
                }

                let Some(connection) = self.user_connections.get_mut(&user.address()) else {
                    continue;
                };

                if !connection
                    .base
                    .world_manager
                    .has_global_entity(global_entity)
                {
                    // entity is not in scope for this connection
                    continue;
                }

                // Send EnableDelegationEntity action through EntityActionEvent system
                info!(
                    "Sending EnableDelegation command for entity: {:?} for user: {:?}",
                    global_entity,
                    user.address()
                );
                connection.base.world_manager.send_enable_delegation(
                    HostType::Server,
                    client_origin.is_some(),
                    global_entity,
                );
            }
        }

        if let Some(client_key) = client_origin {
            self.enable_delegation_client_owned_entity(
                world,
                global_entity,
                world_entity,
                &client_key,
            );
        } else {
            self.global_world_manager
                .entity_enable_delegation(&global_entity);
            world.entity_enable_delegation(
                &self.component_kinds,
                &self.global_entity_map,
                &self.global_world_manager,
                world_entity,
            );
        }
    }

    pub(crate) fn enable_delegation_client_owned_entity<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: &GlobalEntity,
        world_entity: &E,
        client_key: &UserKey,
    ) {
        let Some(entity_owner) = self.global_world_manager.entity_owner(global_entity) else {
            panic!("entity should have an owner at this point");
        };
        let owner_user_key;
        match entity_owner {
            EntityOwner::Client(user_key) => {
                owner_user_key = user_key;
                warn!(
                    "entity should be owned by a public client at this point. Owner is: {:?}",
                    entity_owner
                );

                // publishing here to ensure that the entity is in the correct state ..
                // TODO: this is probably a bad idea somehow! this is hacky
                // instead, should rely on client message coming through at the appropriate time to publish the entity before this..
                let result = self.global_world_manager.entity_publish(&global_entity);
                if result {
                    world.entity_publish(
                        &self.component_kinds,
                        &self.global_entity_map,
                        &self.global_world_manager,
                        world_entity,
                    );
                } else {
                    warn!("failed to publish entity before enabling delegation");
                    return;
                }
            }
            EntityOwner::ClientPublic(user_key) => {
                owner_user_key = user_key;
            }
            _owner => {
                panic!(
                    "entity should be owned by a public client at this point. Owner is: {:?}",
                    entity_owner
                );
            }
        }
        let user_key = owner_user_key;
        self.global_world_manager
            .migrate_entity_to_server(&global_entity);

        // we set this to true immediately since it's already being replicated out to the remote
        self.entity_scope_map.insert(user_key, *global_entity, true);

        // Migrate Entity from Remote -> Host connection
        let Some(user) = self.users.get(&user_key) else {
            panic!("user should exist");
        };
        let Some(connection) = self.user_connections.get_mut(&user.address()) else {
            panic!("connection does not exist")
        };

        // Step 0: Capture old RemoteEntity BEFORE migration (will be needed for MigrateResponse)
        let old_remote_entity = match connection
            .base
            .world_manager
            .entity_converter()
            .global_entity_to_remote_entity(global_entity)
        {
            Ok(entity) => entity,
            Err(_) => {
                panic!(
                    "Entity must exist as RemoteEntity before delegation: {:?}",
                    global_entity
                );
            }
        };

        // Step 1: Migrate entity from RemoteEntity to HostEntity
        // This creates the HostEntity in HostEngine so it can receive commands
        let new_host_entity = match connection
            .base
            .world_manager
            .migrate_entity_remote_to_host(global_entity)
        {
            Ok(entity) => entity,
            Err(e) => {
                panic!("Failed to migrate entity during delegation: {}", e);
            }
        };

        // Step 2: Force the server's HostEntityChannel into Delegated state locally
        // This allows MigrateResponse to be sent (requires Delegated state)
        // We do NOT send EnableDelegation back to the client - they already sent it!
        connection
            .base
            .world_manager
            .host_local_enable_delegation(&new_host_entity);

        // Step 3: Send MigrateResponse to client
        // This will be the FIRST message in the new HostEntityChannel sequence (subcommand_id=0)
        connection.base.world_manager.host_send_migrate_response(
            global_entity,
            &old_remote_entity,
            &new_host_entity,
        );

        self.global_world_manager
            .entity_enable_delegation(&global_entity);
        world.entity_enable_delegation(
            &self.component_kinds,
            &self.global_entity_map,
            &self.global_world_manager,
            world_entity,
        );

        // grant authority to user
        let requester = AuthOwner::from_user_key(Some(client_key));
        let result = self
            .global_world_manager
            .client_request_authority(&global_entity, &requester);
        if result.is_err() {
            panic!("failed to grant authority of client-owned delegated entity to creating user");
        }
    }

    pub(crate) fn entity_disable_delegation<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: &GlobalEntity,
        world_entity: &E,
    ) {
        // TODO: check that entity is eligible for delegation?

        // for any users that have this entity in scope, send an `disable_delegation` message
        {
            // TODO: we can make this more efficient in the future by caching which Entities
            // are in each User's scope
            for (_user_key, user) in self.users.iter() {
                let Some(connection) = self.user_connections.get_mut(&user.address()) else {
                    continue;
                };

                if !connection
                    .base
                    .world_manager
                    .has_global_entity(global_entity)
                {
                    // entity is not in scope for this connection
                    continue;
                }

                // Send DisableDelegationEntity action through EntityActionEvent system
                connection
                    .base
                    .world_manager
                    .send_disable_delegation(global_entity);
            }
        }

        self.global_world_manager
            .entity_disable_delegation(&global_entity);
        world.entity_disable_delegation(world_entity);
    }

    //// Users

    /// Get a User's Socket Address, given the associated UserKey
    pub(crate) fn user_address(&self, user_key: &UserKey) -> Option<SocketAddr> {
        if let Some(user) = self.users.get(user_key) {
            return Some(user.address());
        }
        None
    }

    /// Returns an iterator of all the keys of the [`Room`]s the User belongs to
    pub(crate) fn user_room_keys(&'_ self, user_key: &UserKey) -> Option<Iter<'_, RoomKey>> {
        if let Some(user) = self.users.get(user_key) {
            return Some(user.room_keys().iter());
        }
        return None;
    }

    /// Get an count of how many Rooms the given User is inside
    pub(crate) fn user_rooms_count(&self, user_key: &UserKey) -> Option<usize> {
        if let Some(user) = self.users.get(user_key) {
            return Some(user.room_count());
        }
        return None;
    }

    pub(crate) fn user_disconnect<W: WorldMutType<E>>(
        &mut self,
        user_key: &UserKey,
        world: &mut W,
    ) {
        if self.client_authoritative_entities {
            self.despawn_all_remote_entities(user_key, world);
            if let Some(all_owned_entities) =
                self.global_world_manager.user_all_owned_entities(user_key)
            {
                let copied_entities = all_owned_entities.clone();
                for global_entity in copied_entities {
                    // Only release authority if entity still exists (may have been despawned already)
                    if let Ok(world_entity) = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                    {
                        let _ = self.entity_release_authority(Some(user_key), &world_entity);
                    }
                }
            }
        }
        let user = self.user_delete(user_key);
        self.incoming_world_events
            .push_disconnection(user_key, user.address());
    }

    pub(crate) fn user_queue_disconnect(&mut self, user_key: &UserKey) {
        let Some(user) = self.users.get(user_key) else {
            // User already disconnected, this is fine (disconnect packets may arrive multiple times)
            return;
        };
        let Some(connection) = self.user_connections.get_mut(&user.address()) else {
            // Connection already gone, user is being/has been disconnected
            return;
        };

        // If already marked for disconnect, don't queue again (idempotent)
        if connection.manual_disconnect {
            return;
        }

        connection.manual_disconnect = true;
        // Add to outstanding_disconnects immediately so it gets processed in the next process_all_packets call
        self.outstanding_disconnects.push(*user_key);
    }

    pub(crate) fn user_delete(&mut self, user_key: &UserKey) -> WorldUser {
        let Some(user) = self.users.remove(user_key) else {
            panic!("Attempting to delete non-existent user!");
        };

        let user_addr = user.address();

        info!("deleting authenticated user for {}", user.address());
        self.user_connections.remove(&user_addr);

        self.entity_scope_map.remove_user(user_key);

        // Clean up all user data
        for room_key in user.room_keys() {
            self.rooms
                .get_mut(room_key)
                .unwrap()
                .unsubscribe_user(user_key);
        }

        // remove from bandwidth monitor
        if self.io.bandwidth_monitor_enabled() {
            self.io.deregister_client(&user.address());
        }

        return user;
    }

    /// All necessary cleanup, when they're actually gone...
    pub(crate) fn despawn_all_remote_entities<W: WorldMutType<E>>(
        &mut self,
        user_key: &UserKey,
        world: &mut W,
    ) {
        let Some(user) = self.users.get(user_key) else {
            panic!("Attempting to despawn entities for a nonexistent user");
        };
        let Some(connection) = self.user_connections.get_mut(&user.address()) else {
            panic!("Attempting to despawn entities on a nonexistent connection");
        };

        let remote_global_entities = connection.base.world_manager.remote_entities();
        let entity_events = SharedGlobalWorldManager::despawn_all_entities(
            world,
            &self.global_entity_map,
            &self.global_world_manager,
            remote_global_entities,
        );
        self.process_entity_events(world, user_key, entity_events);
    }

    //// Rooms

    /// Deletes the Room associated with a given RoomKey on the Server.
    /// Returns true if the Room existed.
    pub(crate) fn room_destroy(&mut self, room_key: &RoomKey) -> bool {
        self.room_remove_all_entities(room_key);

        if self.rooms.contains_key(room_key) {
            // TODO: what else kind of cleanup do we need to do here? Scopes?

            // actually remove the room from the collection
            let room = self.rooms.remove(room_key).unwrap();
            for user_key in room.user_keys() {
                self.users.get_mut(user_key).unwrap().uncache_room(room_key);
            }

            true
        } else {
            false
        }
    }

    //////// users

    /// Returns whether or not an User is currently in a specific Room, given
    /// their keys.
    pub(crate) fn room_has_user(&self, room_key: &RoomKey, user_key: &UserKey) -> bool {
        if let Some(room) = self.rooms.get(room_key) {
            return room.has_user(user_key);
        }
        false
    }

    /// Add an User to a Room, given the appropriate RoomKey & UserKey
    /// Entities will only ever be in-scope for Users which are in a
    /// Room with them
    pub(crate) fn room_add_user(&mut self, room_key: &RoomKey, user_key: &UserKey) {
        #[cfg(feature = "e2e_debug")]
        {
            SERVER_ROOM_MOVE_CALLED.fetch_add(1, Ordering::Relaxed);
        }
        if let Some(user) = self.users.get_mut(user_key) {
            if let Some(room) = self.rooms.get_mut(room_key) {
                room.subscribe_user(user_key);
                user.cache_room(room_key);
            }
        }
    }

    /// Removes a User from a Room
    pub(crate) fn room_remove_user(&mut self, room_key: &RoomKey, user_key: &UserKey) {
        #[cfg(feature = "e2e_debug")]
        {
            SERVER_ROOM_MOVE_CALLED.fetch_add(1, Ordering::Relaxed);
        }
        if let Some(user) = self.users.get_mut(user_key) {
            if let Some(room) = self.rooms.get_mut(room_key) {
                room.unsubscribe_user(user_key);
                user.uncache_room(room_key);
            }
        }
    }

    /// Get a count of Users in a given Room
    pub(crate) fn room_users_count(&self, room_key: &RoomKey) -> usize {
        if let Some(room) = self.rooms.get(room_key) {
            return room.users_count();
        }
        0
    }

    /// Returns an iterator of the [`UserKey`] for Users that belong in the Room
    pub(crate) fn room_user_keys(&self, room_key: &RoomKey) -> impl Iterator<Item = &UserKey> {
        let iter = if let Some(room) = self.rooms.get(room_key) {
            Some(room.user_keys())
        } else {
            None
        };
        iter.into_iter().flatten()
    }

    pub(crate) fn room_entities(&self, room_key: &RoomKey) -> impl Iterator<Item = &GlobalEntity> {
        let iter = if let Some(room) = self.rooms.get(room_key) {
            Some(room.entities())
        } else {
            None
        };
        iter.into_iter().flatten()
    }

    /// Sends a message to all connected users in a given Room using a given channel
    pub(crate) fn room_broadcast_message(
        &mut self,
        channel_kind: &ChannelKind,
        room_key: &RoomKey,
        message_box: Box<dyn Message>,
    ) {
        if let Some(room) = self.rooms.get(room_key) {
            let user_keys: Vec<UserKey> = room.user_keys().cloned().collect();
            for user_key in &user_keys {
                self.send_message_inner(user_key, channel_kind, message_box.clone())
            }
        }
    }

    //////// entities

    /// Returns whether or not an Entity is currently in a specific Room, given
    /// their keys.
    pub(crate) fn room_has_entity(&self, room_key: &RoomKey, entity: &GlobalEntity) -> bool {
        let Some(room) = self.rooms.get(room_key) else {
            return false;
        };
        return room.has_entity(entity);
    }

    /// Add an Entity to a Room associated with the given RoomKey.
    /// Entities will only ever be in-scope for Users which are in a Room with
    /// them.
    pub(crate) fn room_add_entity(&mut self, room_key: &RoomKey, world_entity: &E) {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();
        let mut is_some = false;
        if let Some(room) = self.rooms.get_mut(room_key) {
            room.add_entity(&global_entity);
            is_some = true;
        }
        if !is_some {
            return;
        }
        self.entity_room_map
            .entity_add_room(&global_entity, room_key);
    }

    /// Remove an Entity from a Room, associated with the given RoomKey
    pub(crate) fn room_remove_entity(&mut self, room_key: &RoomKey, world_entity: &E) {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();
        if let Some(room) = self.rooms.get_mut(room_key) {
            room.remove_entity(&global_entity, false);
            self.entity_room_map
                .remove_from_room(&global_entity, room_key);
        }
    }

    /// Remove all Entities from a Room, associated with the given RoomKey
    fn room_remove_all_entities(&mut self, room_key: &RoomKey) {
        if let Some(room) = self.rooms.get_mut(room_key) {
            let global_entities: Vec<GlobalEntity> = room.entities().copied().collect();
            for global_entity in global_entities {
                room.remove_entity(&global_entity, false);
                self.entity_room_map
                    .remove_from_room(&global_entity, room_key);
            }
        }
    }

    /// Get a count of Entities in a given Room
    pub(crate) fn room_entities_count(&self, room_key: &RoomKey) -> usize {
        if let Some(room) = self.rooms.get(room_key) {
            return room.entities_count();
        }
        0
    }

    // Private methods

    fn read_data_packet(
        &mut self,
        address: &SocketAddr,
        header: &StandardHeader,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        if header.packet_type != PacketType::Data {
            panic!("Server Error: received non-data packet in data packet handler");
        }

        let Some(connection) = self.user_connections.get_mut(address) else {
            return Ok(());
        };

        #[cfg(feature = "e2e_debug")]
        {
            SERVER_RX_FRAMES.fetch_add(1, Ordering::Relaxed);
        }

        // Process incoming header
        connection.process_incoming_header(header);

        // read client tick
        let client_tick = Tick::de(reader)?;

        let server_tick = self.time_manager.current_tick();

        // process data
        connection.read_packet(
            &self.channel_kinds,
            &self.message_kinds,
            &self.component_kinds,
            self.client_authoritative_entities,
            server_tick,
            client_tick,
            reader,
        )?;

        // Mark that we should send an ACK-only packet
        connection.base.mark_should_send_empty_ack();

        return Ok(());
    }

    fn process_disconnects<W: WorldMutType<E>>(&mut self, world: &mut W) {
        let user_disconnects = std::mem::take(&mut self.outstanding_disconnects);
        for user_key in user_disconnects {
            self.user_disconnect(&user_key, world);
        }
    }

    fn process_packets<W: WorldMutType<E>>(
        &mut self,
        address: &SocketAddr,
        world: &mut W,
        now: &Instant,
    ) {
        // Packets requiring established connection
        let (user_key, entity_events) = {
            let Some(connection) = self.user_connections.get_mut(address) else {
                return;
            };
            (
                connection.user_key,
                connection.process_packets(
                    &self.message_kinds,
                    &self.component_kinds,
                    self.client_authoritative_entities,
                    now,
                    &mut self.global_entity_map,
                    &mut self.global_world_manager,
                    &mut self.global_request_manager,
                    &mut self.global_response_manager,
                    world,
                    &mut self.incoming_world_events,
                ),
            )
        };
        self.process_entity_events(world, &user_key, entity_events);
    }

    fn process_entity_events<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        user_key: &UserKey,
        response_events: Vec<EntityEvent>,
    ) {
        let mut deferred_events = Vec::new();
        for response_event in response_events {
            match response_event {
                EntityEvent::Spawn(global_entity) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.incoming_world_events
                        .push_spawn(user_key, &world_entity);
                    self.global_world_manager
                        .insert_entity_record(&global_entity, EntityOwner::Client(*user_key));
                    let user = self.users.get(user_key).unwrap();
                    let connection = self.user_connections.get_mut(&user.address()).unwrap();
                    connection
                        .base
                        .world_manager
                        .remote_spawn_entity(&global_entity); // TODO: migrate to localworldmanager
                    #[cfg(feature = "e2e_debug")]
                    {
                        SERVER_SPAWN_APPLIED.fetch_add(1, Ordering::Relaxed);
                    }
                }
                EntityEvent::Despawn(global_entity) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.incoming_world_events
                        .push_despawn(user_key, &world_entity);
                    deferred_events.push(EntityEvent::Despawn(global_entity));
                }
                EntityEvent::InsertComponent(global_entity, component_kind) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.incoming_world_events.push_insert(
                        user_key,
                        &world_entity,
                        &component_kind,
                    );
                    self.global_world_manager.insert_component_record(
                        // &self.component_kinds,
                        &global_entity,
                        &component_kind,
                    );
                    let is_public_and_client_owned = self
                        .global_world_manager
                        .entity_is_public_and_client_owned(&global_entity);
                    let is_delegated = self
                        .global_world_manager
                        .entity_is_delegated(&global_entity);

                    if is_public_and_client_owned || is_delegated {
                        world.component_publish(
                            &self.component_kinds,
                            &self.global_entity_map,
                            &self.global_world_manager,
                            &world_entity,
                            &component_kind,
                        );

                        if is_delegated {
                            world.component_enable_delegation(
                                &self.component_kinds,
                                &self.global_entity_map,
                                &self.global_world_manager,
                                &world_entity,
                                &component_kind,
                            );
                        }

                        self.insert_new_component_into_entity_scopes(
                            &global_entity,
                            &component_kind,
                            Some(user_key),
                        );
                    }
                }
                EntityEvent::RemoveComponent(global_entity, component) => {
                    let component_kind = component.kind();
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.incoming_world_events
                        .push_remove(user_key, &world_entity, component);
                    if self
                        .global_world_manager
                        .entity_is_public_and_client_owned(&global_entity)
                        || self
                            .global_world_manager
                            .entity_is_delegated(&global_entity)
                    {
                        self.remove_component_worldless(&world_entity, &component_kind);
                    } else {
                        self.global_world_manager
                            .remove_component_record(&global_entity, &component_kind);
                    }
                }
                EntityEvent::UpdateComponent(_tick, global_entity, component_kind) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.incoming_world_events.push_update(
                        user_key,
                        &world_entity,
                        &component_kind,
                    );
                }
                _ => {
                    deferred_events.push(response_event);
                }
            }
        }

        let mut extra_deferred_events = Vec::new();
        // The reason for deferring these events is that they depend on the operations to the world above
        for response_event in deferred_events {
            match response_event {
                EntityEvent::Publish(global_entity) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.publish_entity(world, &global_entity, &world_entity, false);
                    self.incoming_world_events
                        .push_publish(user_key, &world_entity);
                    
                    // NOTE: Client-owned entities do NOT get auto-granted authority.
                    // Authority/SetAuthority only applies to delegated (server-owned) entities.
                }
                EntityEvent::Unpublish(global_entity) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.unpublish_entity(world, &global_entity, &world_entity, false);
                    self.incoming_world_events
                        .push_unpublish(user_key, &world_entity);
                }
                EntityEvent::EnableDelegation(global_entity) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.entity_enable_delegation(
                        world,
                        &global_entity,
                        &world_entity,
                        Some(*user_key),
                    );
                    self.incoming_world_events
                        .push_delegate(user_key, &world_entity);
                }
                EntityEvent::EnableDelegationResponse(global_entity) => {
                    self.entity_enable_delegation_response(user_key, &global_entity);
                }
                EntityEvent::DisableDelegation(_) => {
                    panic!("Clients should not be able to disable entity delegation.");
                }
                EntityEvent::RequestAuthority(global_entity) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    let _ = self.entity_give_authority(user_key, &world_entity);
                }
                EntityEvent::ReleaseAuthority(global_entity) => {
                    // info!("received release auth entity message!");
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    if self.entity_release_authority(Some(user_key), &world_entity).is_ok() {
                        self.incoming_world_events.push_auth_reset(&world_entity);
                    }
                }
                EntityEvent::SetAuthority(_, _) => {
                    panic!("Clients should not be able to update entity authority.");
                }
                EntityEvent::MigrateResponse(_, _) => {
                    panic!("Clients should not be able to send this message");
                }
                _ => {
                    extra_deferred_events.push(response_event);
                }
            }
        }

        for response_event in extra_deferred_events {
            match response_event {
                EntityEvent::Despawn(global_entity) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.incoming_world_events
                        .push_despawn(user_key, &world_entity);
                    if self
                        .global_world_manager
                        .entity_is_public_and_client_owned(&global_entity)
                        || self
                            .global_world_manager
                            .entity_is_delegated(&global_entity)
                    {
                        // remove from host connection
                        let user = self.users.get(user_key).unwrap();
                        let connection = self.user_connections.get_mut(&user.address()).unwrap();
                        connection
                            .base
                            .world_manager
                            .remote_despawn_entity(&global_entity);

                        self.despawn_entity_worldless(&world_entity);
                    } else {
                        self.global_world_manager
                            .remove_entity_record(&global_entity);
                        self.global_entity_map.despawn_by_global(&global_entity);
                    }
                }
                _ => {
                    panic!("shouldn't happen");
                }
            }
        }
    }

    fn handle_pings(&mut self) {
        // pings
        if self.ping_timer.ringing() {
            self.ping_timer.reset();

            for (user_address, connection) in &mut self.user_connections.iter_mut() {
                // send pings
                if connection.ping_manager.should_send_ping() {
                    let mut writer = BitWriter::new();

                    // write header
                    let _header = connection.base.write_header(PacketType::Ping, &mut writer);

                    // write server tick
                    self.time_manager.current_tick().ser(&mut writer);

                    // write server tick instant
                    self.time_manager.current_tick_instant().ser(&mut writer);

                    // write body
                    connection
                        .ping_manager
                        .write_ping(&mut writer, &self.time_manager);

                    // send packet
                    if self
                        .io
                        .send_packet(user_address, writer.to_packet())
                        .is_err()
                    {
                        // TODO: pass this on and handle above
                        warn!("Server Error: Cannot send ping packet to {}", user_address);
                    }
                    connection.base.mark_sent();
                }
            }
        }
    }

    fn handle_heartbeats(&mut self) {
        // heartbeats
        if self.heartbeat_timer.ringing() {
            self.heartbeat_timer.reset();

            for (user_address, connection) in &mut self.user_connections.iter_mut() {
                // user heartbeats
                if connection.base.should_send_heartbeat() {
                    Self::send_heartbeat_packet(
                        user_address,
                        connection,
                        &self.time_manager,
                        &mut self.io,
                    );
                }
            }
        }
    }

    fn send_heartbeat_packet(
        user_address: &SocketAddr,
        connection: &mut Connection,
        time_manager: &TimeManager,
        io: &mut Io,
    ) {
        // Don't try to refactor this to self.internal_send, doesn't seem to
        // work cause of iter_mut()
        let mut writer = BitWriter::new();

        // write header
        let _header = connection
            .base
            .write_header(PacketType::Heartbeat, &mut writer);

        // write server tick
        time_manager.current_tick().ser(&mut writer);

        // write server tick instant
        time_manager.current_tick_instant().ser(&mut writer);

        // send packet
        if io.send_packet(user_address, writer.to_packet()).is_err() {
            // TODO: pass this on and handle above
            warn!(
                "Server Error: Cannot send heartbeat packet to {}",
                user_address
            );
        }
        connection.base.mark_sent();
    }

    fn handle_empty_acks(&mut self) {
        // empty acks

        for (user_address, connection) in &mut self.user_connections.iter_mut() {
            if connection.base.should_send_empty_ack() {
                Self::send_heartbeat_packet(
                    user_address,
                    connection,
                    &self.time_manager,
                    &mut self.io,
                );
            }
        }
    }

    // Entity Scopes

    fn update_entity_scopes<W: WorldRefType<E>>(&mut self, world: &W) {
        for (_, room) in self.rooms.iter_mut() {
            while let Some((removed_user, removed_global_entity)) = room.pop_entity_removal_queue()
            {
                let Some(user) = self.users.get(&removed_user) else {
                    continue;
                };
                let Some(connection) = self.user_connections.get_mut(&user.address()) else {
                    continue;
                };

                // evaluate whether the Entity really needs to be despawned!
                // what if the Entity shares another Room with this User? It shouldn't be despawned!
                if let Some(entity_rooms) = self
                    .entity_room_map
                    .entity_get_rooms(&removed_global_entity)
                {
                    let user_rooms = user.room_keys();
                    let has_room_in_common = entity_rooms.intersection(user_rooms).next().is_some();
                    if has_room_in_common {
                        continue;
                    }
                }

                // check if host has entity, because it may have been removed from room before despawning, and we don't want to double despawn
                if !connection
                    .base
                    .world_manager
                    .has_global_entity(&removed_global_entity)
                {
                    // entity is not in scope for this connection
                    continue;
                }

                // remove entity from user connection
                connection
                    .base
                    .world_manager
                    .despawn_entity(&removed_global_entity);
                #[cfg(feature = "e2e_debug")]
                {
                    SERVER_SCOPE_DIFF_ENQUEUED.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        for (_, room) in self.rooms.iter_mut() {
            // TODO: we should be able to cache these tuples of keys to avoid building a new
            // list each time
            for user_key in room.user_keys() {
                let Some(user) = self.users.get(user_key) else {
                    continue;
                };
                let Some(connection) = self.user_connections.get_mut(&user.address()) else {
                    continue;
                };
                for global_entity in room.entities() {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    if !world.has_entity(&world_entity) {
                        continue;
                    }
                    if self
                        .global_world_manager
                        .entity_is_public_and_owned_by_user(user_key, global_entity)
                    {
                        // entity is owned by client, but it is public, so we don't need to replicate it
                        continue;
                    }

                    let currently_in_scope = connection
                        .base
                        .world_manager
                        .has_global_entity(global_entity);

                    // Default is true (in-scope) since we're already iterating room entities for room users
                    let should_be_in_scope = if let Some(in_scope) =
                        self.entity_scope_map.get(user_key, global_entity)
                    {
                        *in_scope
                    } else {
                        true
                    };

                    if should_be_in_scope {
                        if currently_in_scope {
                            continue;
                        }
                        let component_kinds = self
                            .global_world_manager
                            .component_kinds(global_entity)
                            .unwrap();
                        // add entity & components to the connections local scope
                        connection
                            .base
                            .world_manager
                            .host_init_entity(global_entity, component_kinds);
                        #[cfg(feature = "e2e_debug")]
                        {
                            SERVER_SCOPE_DIFF_ENQUEUED.fetch_add(1, Ordering::Relaxed);
                        }

                        // if entity is delegated, send message to connection via EntityActionEvent system
                        if !self.global_world_manager.entity_is_delegated(global_entity) {
                            continue;
                        }

                        connection.base.world_manager.send_enable_delegation(
                            HostType::Server,
                            false,
                            global_entity,
                        );
                    } else if currently_in_scope {
                        // remove entity from the connections local scope
                        connection.base.world_manager.despawn_entity(global_entity);
                    }
                }
            }
        }
    }

    fn handle_disconnects(&mut self) {
        // disconnects
        if self.timeout_timer.ringing() {
            self.timeout_timer.reset();

            for (_, connection) in self.user_connections.iter() {
                // Skip manual disconnects - they're already queued by user_queue_disconnect
                if connection.manual_disconnect {
                    continue;
                }
                // TODO: Add timeout detection logic here for non-manual disconnects
                // For now, handle_disconnects only processes manual disconnects which are
                // already queued, so we skip them to avoid double-processing
            }
        }
    }
}

impl<E: Hash + Copy + Eq + Sync + Send> EntityAndGlobalEntityConverter<E> for WorldServer<E> {
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<E, EntityDoesNotExistError> {
        self.global_entity_map
            .global_entity_to_entity(global_entity)
    }

    fn entity_to_global_entity(
        &self,
        world_entity: &E,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.global_entity_map.entity_to_global_entity(world_entity)
    }
}

cfg_if! {
    if #[cfg(feature = "interior_visibility")] {

        use naia_shared::{LocalEntity, OwnedLocalEntity};

        impl<E: Copy + Eq + Hash + Send + Sync> WorldServer<E> {
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
                let user = self.users.get(user_key).expect("User does not exist");
                let connection = self
                    .user_connections
                    .get(&user.address())
                    .expect("User connection does not exist");

                connection.base.world_manager.local_entities()
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
                let world_entity = self.local_to_world_entity(user_key, local_entity)?;
                if !world.has_entity(&world_entity) {
                    return None;
                }
                Some(self.entity(world, &world_entity))
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
                let world_entity = self.local_to_world_entity(user_key, local_entity)?;
                if !world.has_entity(&world_entity) {
                    return None;
                }
                Some(self.entity_mut(world, &world_entity))
            }

            pub(crate) fn local_to_world_entity(
                &self,
                user_key: &UserKey,
                local_entity: &LocalEntity
            ) -> Option<E> {
                let user = self.users.get(user_key)?;
                let connection = self.user_connections.get(&user.address())?;
                let converter = connection.base.world_manager.entity_converter();

                let owned_local_entity: OwnedLocalEntity = (*local_entity).into();
                let global_entity = converter.owned_entity_to_global_entity(&owned_local_entity).ok()?;
                let world_entity = self
                    .global_entity_map
                    .global_entity_to_entity(&global_entity)
                    .ok()?;

                Some(world_entity)
            }

            pub(crate) fn world_to_local_entity(
                &self,
                user_key: &UserKey,
                world_entity: &E,
            ) -> Option<LocalEntity> {
                let global_entity = self.global_entity_map.entity_to_global_entity(world_entity).ok()?;

                let user = self.users.get(user_key)?;
                let connection = self.user_connections.get(&user.address())?;
                let converter = connection.base.world_manager.entity_converter();
                let owned_entity = converter.global_entity_to_owned_entity(&global_entity).ok()?;

                Some(LocalEntity::from(owned_entity))
            }
        }
    }
}
