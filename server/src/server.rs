use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
    net::SocketAddr,
    panic,
    sync::{Arc, RwLock},
};

use naia_server_socket::{Packet, ServerAddrs, Socket};
pub use naia_shared::{
    wrapping_diff, BaseConnection, ConnectionConfig, Instant, KeyGenerator, LocalComponentKey,
    ManagerType, Manifest, NetEntity, PacketReader, PacketType, PropertyMutate, PropertyMutator,
    ProtocolKindType, Protocolize, Replicate, ReplicateSafe, SharedConfig, StandardHeader, Timer,
    Timestamp, WorldMutType, WorldRefType,
};
use slotmap::DenseSlotMap;

use super::{
    connection::Connection,
    entity_ref::{EntityMut, EntityRef},
    entity_scope_map::EntityScopeMap,
    error::NaiaServerError,
    event::Event,
    global_diff_handler::GlobalDiffHandler,
    global_entity_record::GlobalEntityRecord,
    handshake_manager::{HandshakeManager, HandshakeResult},
    io::Io,
    keys::ComponentKey,
    room::{room_key::RoomKey, Room, RoomMut, RoomRef},
    server_config::ServerConfig,
    tick_manager::TickManager,
    user::{user_key::UserKey, User, UserMut, UserRef},
    user_scope::UserScopeMut,
    world_record::WorldRecord,
};

/// A server that uses either UDP or WebRTC communication to send/receive
/// messages to/from connected clients, and syncs registered entities to
/// clients to whom they are in-scope
pub struct Server<P: Protocolize, E: Copy + Eq + Hash> {
    // Config
    manifest: Manifest<P>,
    // Connection
    connection_config: ConnectionConfig,
    socket: Socket,
    io: Io,
    heartbeat_timer: Timer,
    handshake_manager: HandshakeManager<P>,
    // Users
    users: DenseSlotMap<UserKey, User>,
    user_connections: HashMap<SocketAddr, Connection<P, E>>,
    // Rooms
    rooms: DenseSlotMap<RoomKey, Room<E>>,
    // Entities
    world_record: WorldRecord<E, P::Kind>,
    entity_records: HashMap<E, GlobalEntityRecord>,
    entity_scope_map: EntityScopeMap<E>,
    // Components
    diff_handler: Arc<RwLock<GlobalDiffHandler>>,
    // Events
    outstanding_connects: VecDeque<(SocketAddr, UserKey)>,
    outstanding_disconnects: VecDeque<UserKey>,
    outstanding_auths: VecDeque<(UserKey, P)>,
    outstanding_errors: VecDeque<NaiaServerError>,
    // Ticks
    tick_manager: Option<TickManager>,
}

impl<P: Protocolize, E: Copy + Eq + Hash> Server<P, E> {
    /// Create a new Server
    pub fn new(mut server_config: ServerConfig, shared_config: SharedConfig<P>) -> Self {
        server_config.socket_config.link_condition_config =
            shared_config.link_condition_config.clone();

        let connection_config = ConnectionConfig::new(
            server_config.disconnection_timeout_duration,
            server_config.heartbeat_interval,
            server_config.ping_interval,
            server_config.rtt_sample_size,
        );

        let socket = Socket::new(server_config.socket_config);

        let heartbeat_timer = Timer::new(connection_config.heartbeat_interval);

        let tick_manager = {
            if let Some(duration) = shared_config.tick_interval {
                Some(TickManager::new(duration))
            } else {
                None
            }
        };

        Server {
            // Config
            manifest: shared_config.manifest,
            // Connection
            connection_config,
            socket,
            io: Io::new(),
            heartbeat_timer,
            handshake_manager: HandshakeManager::new(server_config.require_auth),
            // Users
            users: DenseSlotMap::with_key(),
            user_connections: HashMap::new(),
            // Rooms
            rooms: DenseSlotMap::with_key(),
            // Entities
            world_record: WorldRecord::new(),
            entity_records: HashMap::new(),
            entity_scope_map: EntityScopeMap::new(),
            // Components
            diff_handler: Arc::new(RwLock::new(GlobalDiffHandler::new())),
            // Events
            outstanding_auths: VecDeque::new(),
            outstanding_connects: VecDeque::new(),
            outstanding_disconnects: VecDeque::new(),
            outstanding_errors: VecDeque::new(),
            // Ticks
            tick_manager,
        }
    }

    /// Listen at the given addresses
    pub fn listen(&mut self, server_addrs: ServerAddrs) {
        self.socket.listen(server_addrs);
        self.io
            .load(self.socket.packet_sender(), self.socket.packet_receiver());
    }

    /// Returns whether or not the Server has initialized correctly and is
    /// listening for Clients
    pub fn is_listening(&self) -> bool {
        self.io.is_loaded()
    }

    /// Must be called regularly, maintains connection to and receives messages
    /// from all Clients
    pub fn receive(&mut self) -> VecDeque<Result<Event<P, E>, NaiaServerError>> {
        let mut events = VecDeque::new();

        // Need to run this to maintain connection with all clients, and receive packets
        // until none left
        self.maintain_socket();

        // new authorizations
        while let Some((user_key, auth_message)) = self.outstanding_auths.pop_front() {
            events.push_back(Ok(Event::Authorization(user_key, auth_message)));
        }

        // new connections
        while let Some((user_address, user_key)) = self.outstanding_connects.pop_front() {
            let mut new_connection = Connection::new(
                &self.connection_config,
                user_address,
                &user_key,
                &self.diff_handler,
            );
            self.handshake_manager
                .send_connect_accept_response(&mut self.io, &mut new_connection);
            self.user_connections.insert(user_address, new_connection);
            events.push_back(Ok(Event::Connection(user_key)));
        }

        // new disconnections
        while let Some(user_key) = self.outstanding_disconnects.pop_front() {
            if let Some(user) = self.delete_user(&user_key) {
                events.push_back(Ok(Event::Disconnection(user_key, user)));
            }
        }

        // TODO: have 1 single queue for messages from all users, as it's
        // possible this current technique unfairly favors the 1st users in
        // self.user_connections
        let server_tick_opt = self.server_tick();
        for (_, connection) in self.user_connections.iter_mut() {
            //receive messages from anyone
            while let Some(message) = connection.get_incoming_message() {
                events.push_back(Ok(Event::Message(connection.user_key, message)));
            }
            //receive entity messages from anyone
            if let Some(server_tick) = server_tick_opt {
                while let Some((entity, message)) =
                    connection.get_incoming_entity_message(server_tick)
                {
                    events.push_back(Ok(Event::MessageEntity(
                        connection.user_key,
                        entity,
                        message,
                    )));
                }
            }
        }

        // new errors
        while let Some(err) = self.outstanding_errors.pop_front() {
            events.push_back(Err(err));
        }

        // tick event
        if let Some(tick_manager) = &mut self.tick_manager {
            if tick_manager.should_tick() {
                events.push_back(Ok(Event::Tick));
            }
        }

        events
    }

    // Connections

    /// Accepts an incoming Client User, allowing them to establish a connection
    /// with the Server
    pub fn accept_connection(&mut self, user_key: &UserKey) {
        if let Some(user) = self.users.get(*user_key) {
            self.outstanding_connects
                .push_back((user.address, *user_key));
        }
    }

    /// Rejects an incoming Client User, terminating their attempt to establish
    /// a connection with the Server
    pub fn reject_connection(&mut self, user_key: &UserKey) {
        self.delete_user(user_key);
    }

    // Messages

    /// Queues up an Message to be sent to the Client associated with a given
    /// UserKey
    pub fn send_message<R: ReplicateSafe<P>>(
        &mut self,
        user_key: &UserKey,
        message: &R,
        guaranteed_delivery: bool,
    ) {
        if let Some(user) = self.users.get(*user_key) {
            if let Some(connection) = self.user_connections.get_mut(&user.address) {
                connection.send_message(message, guaranteed_delivery);
            }
        }
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
                for entity in room.entities() {
                    list.push((room_key, *user_key, *entity));
                }
            }
        }

        return list;
    }

    /// Sends all update messages to all Clients. If you don't call this
    /// method, the Server will never communicate with it's connected
    /// Clients
    pub fn send_all_updates<W: WorldRefType<P, E>>(&mut self, world: W) {
        // update entity scopes
        self.update_entity_scopes(&world);

        // loop through all connections, send packet
        let server_tick = self.server_tick().unwrap_or(0);
        for (address, connection) in self.user_connections.iter_mut() {
            connection.collect_component_updates(&self.world_record);
            let mut sent = false;
            while let Some(payload) =
                connection.get_outgoing_packet(&world, &self.world_record, server_tick)
            {
                self.io.send_packet(Packet::new_raw(*address, payload));
                sent = true;
            }
            if sent {
                connection.mark_sent();
            }
        }
    }

    // Entities

    /// Creates a new Entity and returns an EntityMut which can be used for
    /// further operations on the Entity
    pub fn spawn_entity<'s, W: WorldMutType<P, E>>(
        &'s mut self,
        mut world: W,
    ) -> EntityMut<'s, P, E, W> {
        let entity = world.spawn_entity();
        self.spawn_entity_init(&entity);

        return EntityMut::new(self, world, &entity);
    }

    /// Creates a new Entity with a specific id
    pub fn spawn_entity_at<'s>(&'s mut self, entity: &E) {
        self.spawn_entity_init(&entity);
    }

    /// Retrieves an EntityRef that exposes read-only operations for the
    /// Entity.
    /// Panics if the Entity does not exist.
    pub fn entity<'s, W: WorldRefType<P, E>>(&'s self, world: W, entity: &E) -> EntityRef<P, E, W> {
        if world.has_entity(entity) {
            return EntityRef::new(world, &entity);
        }
        panic!("No Entity exists for given Key!");
    }

    /// Retrieves an EntityMut that exposes read and write operations for the
    /// Entity.
    /// Panics if the Entity does not exist.
    pub fn entity_mut<'s, 'w, W: WorldMutType<P, E>>(
        &'s mut self,
        world: W,
        entity: &E,
    ) -> EntityMut<'s, P, E, W> {
        if world.has_entity(entity) {
            return EntityMut::new(self, world, &entity);
        }
        panic!("No Entity exists for given Key!");
    }

    /// Gets a Vec of all Entities in the given World
    pub fn entities<W: WorldRefType<P, E>>(&self, world: W) -> Vec<E> {
        return world.entities();
    }

    // Users

    /// Returns whether or not a User exists for the given RoomKey
    pub fn user_exists(&self, user_key: &UserKey) -> bool {
        return self.users.contains_key(*user_key);
    }

    /// Retrieves an UserRef that exposes read-only operations for the User
    /// associated with the given UserKey.
    /// Panics if the user does not exist.
    pub fn user(&self, user_key: &UserKey) -> UserRef<P, E> {
        if self.users.contains_key(*user_key) {
            return UserRef::new(self, &user_key);
        }
        panic!("No User exists for given Key!");
    }

    /// Retrieves an UserMut that exposes read and write operations for the User
    /// associated with the given UserKey.
    /// Returns None if the user does not exist.
    pub fn user_mut(&mut self, user_key: &UserKey) -> UserMut<P, E> {
        if self.users.contains_key(*user_key) {
            return UserMut::new(self, &user_key);
        }
        panic!("No User exists for given Key!");
    }

    /// Return a list of all currently connected Users' keys
    pub fn user_keys(&self) -> Vec<UserKey> {
        let mut output = Vec::new();

        for (user_key, _) in self.users.iter() {
            output.push(user_key);
        }

        return output;
    }

    /// Get the number of Users currently connected
    pub fn users_count(&self) -> usize {
        return self.users.len();
    }

    /// Returns a UserScopeMut, which is used to include/exclude Entities for a
    /// given User
    pub fn user_scope(&mut self, user_key: &UserKey) -> UserScopeMut<P, E> {
        if self.users.contains_key(*user_key) {
            return UserScopeMut::new(self, &user_key);
        }
        panic!("No User exists for given Key!");
    }

    /// Returns whether a given User has a particular Entity in-scope currently
    pub fn user_scope_has_entity(&self, user_key: &UserKey, entity: &E) -> bool {
        if let Some(user) = self.users.get(*user_key) {
            if let Some(user_connection) = self.user_connections.get(&user.address) {
                return user_connection.has_entity(entity);
            }
        }

        return false;
    }

    // Rooms

    /// Creates a new Room on the Server and returns a corresponding RoomMut,
    /// which can be used to add users/entities to the room or retrieve its
    /// key
    pub fn make_room(&mut self) -> RoomMut<P, E> {
        let new_room = Room::new();
        let room_key = self.rooms.insert(new_room);
        return RoomMut::new(self, &room_key);
    }

    /// Returns whether or not a Room exists for the given RoomKey
    pub fn room_exists(&self, room_key: &RoomKey) -> bool {
        return self.rooms.contains_key(*room_key);
    }

    /// Retrieves an RoomMut that exposes read and write operations for the
    /// Room associated with the given RoomKey.
    /// Panics if the room does not exist.
    pub fn room(&self, room_key: &RoomKey) -> RoomRef<P, E> {
        if self.rooms.contains_key(*room_key) {
            return RoomRef::new(self, room_key);
        }
        panic!("No Room exists for given Key!");
    }

    /// Retrieves an RoomMut that exposes read and write operations for the
    /// Room associated with the given RoomKey.
    /// Panics if the room does not exist.
    pub fn room_mut(&mut self, room_key: &RoomKey) -> RoomMut<P, E> {
        if self.rooms.contains_key(*room_key) {
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

        return output;
    }

    /// Get a count of how many Rooms currently exist
    pub fn rooms_count(&self) -> usize {
        self.rooms.len()
    }

    // Ticks

    /// Gets the last received tick from the Client
    pub fn client_tick(&self, user_key: &UserKey) -> Option<u16> {
        if let Some(user) = self.users.get(*user_key) {
            if let Some(user_connection) = self.user_connections.get(&user.address) {
                return Some(user_connection.get_last_received_tick());
            }
        }
        return None;
    }

    /// Gets the current tick of the Server
    pub fn server_tick(&self) -> Option<u16> {
        return self
            .tick_manager
            .as_ref()
            .map(|tick_manager| tick_manager.get_tick());
    }

    // Crate-Public methods

    //// Entities

    /// Despawns the Entity, if it exists.
    /// This will also remove all of the Entity’s Components.
    /// Returns true if the Entity is successfully despawned and false if the
    /// Entity does not exist.
    pub(crate) fn despawn_entity<W: WorldMutType<P, E>>(&mut self, world: &mut W, entity: &E) {
        if !world.has_entity(entity) {
            panic!("attempted to de-spawn nonexistent entity");
        }

        // TODO: we can make this more efficient in the future by caching which Entities
        // are in each User's scope
        for (_, user_connection) in self.user_connections.iter_mut() {
            //remove entity from user connection
            user_connection.despawn_entity(&self.world_record, entity);
        }

        // Clean up associated components
        for component_key in self.world_record.get_component_keys(entity) {
            self.component_cleanup(&component_key);
        }

        // Remove from ECS Record
        self.world_record.despawn_entity(entity);

        // Delete from world
        world.despawn_entity(entity);

        self.entity_scope_map.remove_entity(entity);
        self.entity_records.remove(entity);
    }

    //// Entity Scopes

    pub(crate) fn user_scope_set_entity(
        &mut self,
        user_key: &UserKey,
        entity: &E,
        is_contained: bool,
    ) {
        self.entity_scope_map
            .insert(*user_key, *entity, is_contained);
    }

    //// Components

    /// Adds a Component to an Entity
    pub(crate) fn insert_component<R: ReplicateSafe<P>, W: WorldMutType<P, E>>(
        &mut self,
        world: &mut W,
        entity: &E,
        mut component_ref: R,
    ) {
        if !world.has_entity(entity) {
            panic!("attempted to add component to non-existent entity");
        }

        let component_kind = component_ref.get_kind();

        if world.has_component_of_kind(entity, &component_kind) {
            panic!(
                "attempted to add component to entity which already has one of that type! \
                   an entity is not allowed to have more than 1 type of component at a time."
            )
        }

        // generate unique component key
        let component_key: ComponentKey = self.component_init(entity, &mut component_ref);

        // actually insert component into world
        world.insert_component(entity, component_ref);

        // add component to connections already tracking entity
        for (_, user_connection) in self.user_connections.iter_mut() {
            if user_connection.has_entity(entity) {
                // insert component into user's connection
                user_connection.insert_component(&self.world_record, &component_key);
            }
        }
    }

    /// Removes a Component from an Entity
    pub(crate) fn remove_component<R: Replicate<P>, W: WorldMutType<P, E>>(
        &mut self,
        world: &mut W,
        entity: &E,
    ) -> Option<R> {
        // get component key from type
        let component_kind = P::kind_of::<R>();
        let component_key = self
            .world_record
            .get_key_from_type(entity, &component_kind)
            .expect("component does not exist!");

        // clean up component on all connections
        // TODO: should be able to make this more efficient by caching for every Entity
        // which scopes they are part of
        for (_, user_connection) in self.user_connections.iter_mut() {
            //remove component from user connection
            user_connection.remove_component(&component_key);
        }

        // cleanup all other loose ends
        self.component_cleanup(&component_key);

        // remove from world
        return world.remove_component::<R>(entity);
    }

    //// Users

    /// Get a User's Socket Address, given the associated UserKey
    pub(crate) fn get_user_address(&self, user_key: &UserKey) -> Option<SocketAddr> {
        if let Some(user) = self.users.get(*user_key) {
            return Some(user.address);
        }
        return None;
    }

    pub(crate) fn user_force_disconnect(&mut self, user_key: &UserKey) {
        self.outstanding_disconnects.push_back(*user_key);
    }

    /// All necessary cleanup, when they're actually gone...
    pub(crate) fn delete_user(&mut self, user_key: &UserKey) -> Option<User> {
        if let Some(user) = self.users.remove(*user_key) {
            if let Some(_) = self.user_connections.remove(&user.address) {
                self.entity_scope_map.remove_user(user_key);
                self.handshake_manager.delete_user(&user.address);

                // TODO: cache this?
                // Clean up all user data
                for (_, room) in self.rooms.iter_mut() {
                    room.unsubscribe_user(&user_key);
                }

                return Some(user);
            }
        }

        return None;
    }

    //// Rooms

    /// Deletes the Room associated with a given RoomKey on the Server.
    /// Returns true if the Room existed.
    pub(crate) fn room_destroy(&mut self, room_key: &RoomKey) -> bool {
        if self.rooms.contains_key(*room_key) {
            // remove all entities from the entity_room_map
            for entity in self.rooms.get(*room_key).unwrap().entities() {
                if let Some(record) = self.entity_records.get_mut(entity) {
                    record.room_key = None;
                }
            }

            // TODO: what else kind of cleanup do we need to do here? Scopes?

            // actually remove the room from the collection
            self.rooms.remove(*room_key);

            return true;
        } else {
            return false;
        }
    }

    //////// users

    /// Returns whether or not an User is currently in a specific Room, given
    /// their keys.
    pub(crate) fn room_has_user(&self, room_key: &RoomKey, user_key: &UserKey) -> bool {
        if let Some(room) = self.rooms.get(*room_key) {
            return room.has_user(user_key);
        }
        return false;
    }

    /// Add an User to a Room, given the appropriate RoomKey & UserKey
    /// Entities will only ever be in-scope for Users which are in a
    /// Room with them
    pub(crate) fn room_add_user(&mut self, room_key: &RoomKey, user_key: &UserKey) {
        if let Some(room) = self.rooms.get_mut(*room_key) {
            room.subscribe_user(user_key);
        }
    }

    /// Removes a User from a Room
    pub(crate) fn room_remove_user(&mut self, room_key: &RoomKey, user_key: &UserKey) {
        if let Some(room) = self.rooms.get_mut(*room_key) {
            room.unsubscribe_user(user_key);
        }
    }

    /// Get a count of Users in a given Room
    pub(crate) fn room_users_count(&self, room_key: &RoomKey) -> usize {
        if let Some(room) = self.rooms.get(*room_key) {
            return room.users_count();
        }
        return 0;
    }

    //////// entities

    /// Returns whether or not an Entity is currently in a specific Room, given
    /// their keys.
    pub(crate) fn room_has_entity(&self, room_key: &RoomKey, entity: &E) -> bool {
        if let Some(entity_record) = self.entity_records.get(entity) {
            if let Some(actual_room_key) = entity_record.room_key {
                return *room_key == actual_room_key;
            }
        }
        return false;
    }

    /// Add an Entity to a Room associated with the given RoomKey.
    /// Entities will only ever be in-scope for Users which are in a Room with
    /// them.
    pub(crate) fn room_add_entity(&mut self, room_key: &RoomKey, entity: &E) {
        if let Some(entity_record) = self.entity_records.get_mut(entity) {
            if entity_record.room_key.is_some() {
                panic!("Entity already belongs to a Room! Remove the Entity from the Room before adding it to a new Room.");
            }

            if let Some(room) = self.rooms.get_mut(*room_key) {
                room.add_entity(entity);
                entity_record.room_key = Some(*room_key);
            }
        }
    }

    /// Remove an Entity from a Room, associated with the given RoomKey
    pub(crate) fn room_remove_entity(&mut self, room_key: &RoomKey, entity: &E) {
        if let Some(room) = self.rooms.get_mut(*room_key) {
            if room.remove_entity(entity) {
                if let Some(entity_record) = self.entity_records.get_mut(entity) {
                    entity_record.room_key = None;
                }
            }
        }
    }

    /// Get a count of Entities in a given Room
    pub(crate) fn room_entities_count(&self, room_key: &RoomKey) -> usize {
        if let Some(room) = self.rooms.get(*room_key) {
            return room.entities_count();
        }
        return 0;
    }

    // Messages

    /// Sends a Message to a User, associated with a given Entity, once that
    /// Entity is in-scope
    pub(crate) fn send_entity_message<R: ReplicateSafe<P>>(
        &mut self,
        user_key: &UserKey,
        entity: &E,
        message: &R,
    ) {
        if let Some(user) = self.users.get(*user_key) {
            if let Some(connection) = self.user_connections.get_mut(&user.address) {
                connection.send_entity_message(entity, message);
            }
        }
    }

    // Private methods

    fn maintain_socket(&mut self) {
        // heartbeats
        if self.heartbeat_timer.ringing() {
            self.heartbeat_timer.reset();

            let server_tick = self.server_tick().unwrap_or(0);
            for (user_address, connection) in self.user_connections.iter_mut() {
                if connection.should_drop() {
                    self.outstanding_disconnects.push_back(connection.user_key);
                    continue;
                }

                if connection.should_send_heartbeat() {
                    // Don't try to refactor this to self.internal_send, doesn't seem to
                    // work cause of iter_mut()
                    let payload =
                        connection.process_outgoing_header(server_tick, PacketType::Heartbeat, &[]);
                    self.io.send_packet(Packet::new_raw(*user_address, payload));
                    connection.mark_sent();
                }
            }
        }

        //receive socket events
        loop {
            match self.io.receive_packet() {
                Ok(Some(packet)) => {
                    let address = packet.address();

                    if let Some(user_connection) = self.user_connections.get_mut(&address) {
                        user_connection.mark_heard();
                    }

                    let (header, payload) = StandardHeader::read(packet.payload());

                    match header.packet_type() {
                        PacketType::ClientChallengeRequest => {
                            let server_tick = self.server_tick().unwrap_or(0);
                            self.handshake_manager.receive_challenge_request(
                                &mut self.io,
                                server_tick,
                                &address,
                                &payload,
                            )
                        }
                        PacketType::ClientConnectRequest => {
                            if let Some(mut connection) = self.user_connections.get_mut(&address) {
                                self.handshake_manager.receive_old_connect_request(
                                    &mut self.io,
                                    &self.world_record,
                                    &mut connection,
                                    &header,
                                    &payload,
                                );
                            } else {
                                match self.handshake_manager.receive_new_connect_request(
                                    &self.manifest,
                                    &address,
                                    &payload,
                                ) {
                                    HandshakeResult::AuthUser(auth_message) => {
                                        let user = User::new(address);
                                        let user_key = self.users.insert(user);
                                        self.outstanding_auths.push_back((user_key, auth_message));
                                    }
                                    HandshakeResult::ConnectUser => {
                                        let user = User::new(address);
                                        let user_key = self.users.insert(user);
                                        self.accept_connection(&user_key);
                                    }
                                    HandshakeResult::Invalid => {
                                        // do nothing
                                    }
                                }
                            }
                        }
                        PacketType::Disconnect => {
                            if let Some(mut connection) = self.user_connections.get_mut(&address) {
                                if self
                                    .handshake_manager
                                    .verify_disconnect_request(&mut connection, &payload)
                                {
                                    self.outstanding_disconnects.push_back(connection.user_key);
                                }
                            }
                        }
                        PacketType::Data => {
                            let server_tick_opt = self.server_tick();
                            match self.user_connections.get_mut(&address) {
                                Some(connection) => {
                                    connection.process_incoming_header(&self.world_record, &header);
                                    connection.process_incoming_data(
                                        server_tick_opt,
                                        &self.manifest,
                                        &payload,
                                    );
                                }
                                None => {
                                    warn!("received data from unauthenticated client: {}", address);
                                }
                            }
                        }
                        PacketType::Heartbeat => {
                            match self.user_connections.get_mut(&address) {
                                Some(connection) => {
                                    // Still need to do this so that proper notify
                                    // events fire based on the heartbeat header
                                    connection.process_incoming_header(&self.world_record, &header);
                                }
                                None => {
                                    warn!(
                                        "received heartbeat from unauthenticated client: {}",
                                        address
                                    );
                                }
                            }
                        }
                        PacketType::Ping => {
                            let server_tick = self.server_tick().unwrap_or(0);
                            match self.user_connections.get_mut(&address) {
                                Some(connection) => {
                                    connection.process_incoming_header(&self.world_record, &header);
                                    let ping_payload = connection.process_ping(&payload);
                                    let payload_with_header = connection.process_outgoing_header(
                                        server_tick,
                                        PacketType::Pong,
                                        &ping_payload,
                                    );
                                    self.io.send_packet(Packet::new_raw(
                                        connection.address(),
                                        payload_with_header,
                                    ));
                                    connection.mark_sent();
                                }
                                None => {
                                    warn!("received ping from unauthenticated client: {}", address);
                                }
                            }
                        }
                        PacketType::ServerChallengeResponse
                        | PacketType::ServerConnectResponse
                        | PacketType::Pong
                        | PacketType::Unknown => {
                            // do nothing
                        }
                    }
                }
                Ok(None) => {
                    // No more packets, break loop
                    break;
                }
                Err(error) => {
                    self.outstanding_errors
                        .push_back(NaiaServerError::Wrapped(Box::new(error)));
                }
            }
        }
    }

    // Entity Helpers

    fn spawn_entity_init(&mut self, entity: &E) {
        self.world_record.spawn_entity(entity);
        self.entity_records
            .insert(*entity, GlobalEntityRecord::new());
    }

    // Entity Scopes

    fn update_entity_scopes<W: WorldRefType<P, E>>(&mut self, world: &W) {
        for (_, room) in self.rooms.iter_mut() {
            while let Some((removed_user, removed_entity)) = room.pop_entity_removal_queue() {
                if let Some(user) = self.users.get(removed_user) {
                    if let Some(user_connection) = self.user_connections.get_mut(&user.address) {
                        //remove entity from user connection
                        user_connection.despawn_entity(&self.world_record, &removed_entity);
                    }
                }
            }

            // TODO: we should be able to cache these tuples of keys to avoid building a new
            // list each time
            for user_key in room.user_keys() {
                for entity in room.entities() {
                    if world.has_entity(entity) {
                        if let Some(user) = self.users.get(*user_key) {
                            if let Some(user_connection) =
                                self.user_connections.get_mut(&user.address)
                            {
                                let currently_in_scope = user_connection.has_entity(entity);

                                let should_be_in_scope: bool;
                                if let Some(in_scope) = self.entity_scope_map.get(user_key, entity)
                                {
                                    should_be_in_scope = *in_scope;
                                } else {
                                    should_be_in_scope = false;
                                }

                                if should_be_in_scope {
                                    if !currently_in_scope {
                                        // add entity to the connections local scope
                                        user_connection.spawn_entity(&self.world_record, entity);
                                    }
                                } else {
                                    if currently_in_scope {
                                        // remove entity from the connections local scope
                                        user_connection.despawn_entity(&self.world_record, entity);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Component Helpers

    fn component_init<R: ReplicateSafe<P>>(
        &mut self,
        entity: &E,
        component_ref: &mut R,
    ) -> ComponentKey {
        let component_key = self
            .world_record
            .add_component(entity, &component_ref.get_kind());

        let diff_mask_length: u8 = component_ref.get_diff_mask_size();

        let mut_sender = self
            .diff_handler
            .as_ref()
            .write()
            .expect("DiffHandler should be initialized")
            .register_component(&component_key, diff_mask_length);

        let prop_mutator = PropertyMutator::new(mut_sender);

        component_ref.set_mutator(&prop_mutator);

        return component_key;
    }

    fn component_cleanup(&mut self, component_key: &ComponentKey) {
        self.world_record.remove_component(component_key);
        self.diff_handler
            .as_ref()
            .write()
            .expect("Haven't initialized DiffHandler")
            .deregister_component(component_key);
    }
}
