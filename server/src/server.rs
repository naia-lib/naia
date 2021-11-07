use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
    net::SocketAddr,
    panic,
    sync::{Arc, RwLock},
};

use byteorder::{BigEndian, WriteBytesExt};
use ring::{hmac, rand};
use slotmap::DenseSlotMap;

use naia_server_socket::{
    NaiaServerSocketError, Packet, PacketReceiver, PacketSender, ServerAddrs, Socket,
};

pub use naia_shared::{
    wrapping_diff, Connection, ConnectionConfig, Instant, KeyGenerator, LocalComponentKey,
    ManagerType, Manifest, PacketReader, PacketType, PropertyMutate, PropertyMutator,
    ProtocolKindType, ProtocolType, Replicate, ReplicateSafe, SharedConfig, StandardHeader, Timer,
    Timestamp, WorldMutType, WorldRefType,
};

use super::{
    client_connection::ClientConnection,
    entity_ref::{EntityMut, EntityRef, WorldlessEntityMut},
    entity_scope_map::EntityScopeMap,
    error::NaiaServerError,
    event::Event,
    global_diff_handler::GlobalDiffHandler,
    global_entity_record::GlobalEntityRecord,
    keys::ComponentKey,
    room::{room_key::RoomKey, Room, RoomMut, RoomRef},
    server_config::ServerConfig,
    tick_manager::TickManager,
    user::{user_key::UserKey, UserRecord, UserMut, UserRef, User},
    user_scope::UserScopeMut,
    world_record::WorldRecord,
};

/// A server that uses either UDP or WebRTC communication to send/receive
/// messages to/from connected clients, and syncs registered entities to
/// clients to whom they are in-scope
pub struct Server<P: ProtocolType, E: Copy + Eq + Hash> {
    // Config
    manifest: Manifest<P>,
    // Connection
    connection_config: ConnectionConfig,
    socket: Socket,
    io: Io,
    heartbeat_timer: Timer,
    connection_hash_key: hmac::Key,
    require_auth: bool,
    // Users
    user_records: DenseSlotMap<UserKey, UserRecord<E>>,
    address_to_user_key_map: HashMap<SocketAddr, UserKey>,
    client_connections: HashMap<UserKey, ClientConnection<P, E>>,
    // Rooms
    rooms: DenseSlotMap<RoomKey, Room<E>>,
    // Entities
    world_record: WorldRecord<E, P::Kind>,
    entity_records: HashMap<E, GlobalEntityRecord>,
    entity_scope_map: EntityScopeMap<E>,
    // Components
    diff_handler: Arc<RwLock<GlobalDiffHandler>>,
    // Events
    outstanding_connects: VecDeque<UserKey>,
    outstanding_disconnects: VecDeque<UserKey>,
    outstanding_auths: VecDeque<(UserKey, P)>,
    outstanding_errors: VecDeque<NaiaServerError>,
    // Ticks
    tick_manager: Option<TickManager>,
}

impl<P: ProtocolType, E: Copy + Eq + Hash> Server<P, E> {
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

        let clients_map = HashMap::new();
        let heartbeat_timer = Timer::new(connection_config.heartbeat_interval);

        let connection_hash_key =
            hmac::Key::generate(hmac::HMAC_SHA256, &rand::SystemRandom::new()).unwrap();

        let tick_manager = {
            if let Some(duration) = shared_config.tick_interval {
                Some(TickManager::new(duration))
            } else {
                None
            }
        };

        let require_auth = server_config.require_auth;

        Server {
            // Config
            manifest: shared_config.manifest,
            // Connection
            connection_config,
            socket,
            io: Io::new(),
            heartbeat_timer,
            connection_hash_key,
            require_auth,
            // Users
            user_records: DenseSlotMap::with_key(),
            address_to_user_key_map: HashMap::new(),
            client_connections: clients_map,
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
        self.io.load(
            self.socket.get_packet_sender(),
            self.socket.get_packet_receiver(),
        );
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
        while let Some(user_key) = self.outstanding_connects.pop_front() {
            if let Some(user_record) = self.user_records.get(user_key) {

                let user_address = user_record.user.address;

                self.address_to_user_key_map.insert(user_address, user_key);

                let mut new_connection = ClientConnection::new(
                    user_address,
                    &self.connection_config,
                    &self.diff_handler,
                );

                // not sure if I should uncomment this...
                //new_connection.process_incoming_header(&header);

                // send connect accept message //
                let payload = new_connection.process_outgoing_header(
                    None,
                    0,
                    PacketType::ServerConnectResponse,
                    &[],
                );
                self.io
                    .send_packet(Packet::new_raw(new_connection.get_address(), payload));
                new_connection.mark_sent();
                /////////////////////////////////

                self.client_connections.insert(user_key, new_connection);

                events.push_back(Ok(Event::Connection(user_key)));
            }
        }

        // new disconnections
        while let Some(user_key) = self.outstanding_disconnects.pop_front() {
            if let Some(user) = self.delete_user(&user_key) {
                events.push_back(Ok(Event::Disconnection(user_key, user)));
            }
        }

        // TODO: have 1 single queue for commands/messages from all users, as it's
        // possible this current technique unfairly favors the 1st users in
        // self.client_connections
        let server_tick_opt = self.server_tick();
        for (user_key, connection) in self.client_connections.iter_mut() {
            //receive commands from anyone
            if let Some(server_tick) = server_tick_opt {
                while let Some((prediction_key, command)) =
                    connection.get_incoming_command(server_tick)
                {
                    events.push_back(Ok(Event::Command(*user_key, prediction_key, command)));
                }
            }
            //receive messages from anyone
            while let Some(message) = connection.get_incoming_message() {
                events.push_back(Ok(Event::Message(*user_key, message)));
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
        self.outstanding_connects.push_back(*user_key);
    }

    /// Rejects an incoming Client User, terminating their attempt to establish
    /// a connection with the Server
    pub fn reject_connection(&mut self, user_key: &UserKey) {
        self.delete_user(user_key);
    }

    // Messages

    /// Queues up an Message to be sent to the Client associated with a given
    /// UserKey
    pub fn queue_message<R: ReplicateSafe<P>>(
        &mut self,
        user_key: &UserKey,
        message: &R,
        guaranteed_delivery: bool,
    ) {
        if let Some(connection) = self.client_connections.get_mut(user_key) {
            connection.queue_message(message, guaranteed_delivery);
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
        let server_tick_opt = self.server_tick();
        for (user_key, connection) in self.client_connections.iter_mut() {
            if let Some(user_record) = self.user_records.get(*user_key) {
                connection.collect_component_updates(&self.world_record);
                while let Some(payload) =
                    connection.get_outgoing_packet(&world, &self.world_record, server_tick_opt)
                {
                    self.io.send_packet(Packet::new_raw(user_record.user.address, payload));
                    connection.mark_sent();
                }
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
    pub fn spawn_entity_at<'s>(&'s mut self, entity: &E) -> WorldlessEntityMut<'s, P, E> {
        self.spawn_entity_init(&entity);

        return WorldlessEntityMut::new(self, entity);
    }

    /// Retrieves an EntityRef that exposes read-only operations for the
    /// Entity.
    /// Panics if the Entity does not exist.
    pub fn entity<'s, W: WorldRefType<P, E>>(
        &'s self,
        world: W,
        entity: &E,
    ) -> EntityRef<'s, P, E, W> {
        if world.has_entity(entity) {
            return EntityRef::new(self, world, &entity);
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

    /// Retrieves a WorldlessEntityMut that exposes read and write operations
    /// on the Entity, but with no references allowed to the World.
    /// This is a very niche use case.
    /// Panics if the Entity does not exist.
    pub fn worldless_entity_mut<'s>(&'s mut self, entity: &E) -> WorldlessEntityMut<'s, P, E> {
        return WorldlessEntityMut::new(self, &entity);
    }

    /// Gets a Vec of all Entities in the given World
    pub fn entities<W: WorldRefType<P, E>>(&self, world: &W) -> Vec<E> {
        return world.entities();
    }

    // Users

    /// Returns whether or not a User exists for the given RoomKey
    pub fn user_exists(&self, user_key: &UserKey) -> bool {
        return self.user_records.contains_key(*user_key);
    }

    /// Retrieves an UserRef that exposes read-only operations for the User
    /// associated with the given UserKey.
    /// Panics if the user does not exist.
    pub fn user(&self, user_key: &UserKey) -> UserRef<P, E> {
        if self.user_records.contains_key(*user_key) {
            return UserRef::new(self, &user_key);
        }
        panic!("No User exists for given Key!");
    }

    /// Retrieves an UserMut that exposes read and write operations for the User
    /// associated with the given UserKey.
    /// Returns None if the user does not exist.
    pub fn user_mut(&mut self, user_key: &UserKey) -> UserMut<P, E> {
        if self.user_records.contains_key(*user_key) {
            return UserMut::new(self, &user_key);
        }
        panic!("No User exists for given Key!");
    }

    /// Return a list of all currently connected Users' keys
    pub fn user_keys(&self) -> Vec<UserKey> {
        let mut output = Vec::new();

        for (user_key, _) in self.user_records.iter() {
            output.push(user_key);
        }

        return output;
    }

    /// Get the number of Users currently connected
    pub fn users_count(&self) -> usize {
        return self.user_records.len();
    }

    /// Returns a UserScopeMut, which is used to include/exclude Entities for a
    /// given User
    pub fn user_scope(&mut self, user_key: &UserKey) -> UserScopeMut<P, E> {
        if self.user_records.contains_key(*user_key) {
            return UserScopeMut::new(self, &user_key);
        }
        panic!("No User exists for given Key!");
    }

    /// Returns whether a given User has a particular Entity in-scope currently
    pub fn user_scope_has_entity(&self, user_key: &UserKey, entity: &E) -> bool {
        if let Some(client_connection) = self.client_connections.get(user_key) {
            return client_connection.has_entity(entity);
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
        if let Some(client_connection) = self.client_connections.get(user_key) {
            return Some(client_connection.get_last_received_tick());
        }
        return None;
    }

    /// Gets the current tick of the Server
    pub fn server_tick(&self) -> Option<u16> {
        if let Some(tick_manager) = &self.tick_manager {
            return Some(tick_manager.get_tick());
        } else {
            None
        }
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
        // Clean up ownership if applicable
        if self.entity_has_owner(entity) {
            self.entity_disown(entity);
        }

        // TODO: we can make this more efficient in the future by caching which Entities
        // are in each User's scope
        for (user_key, _) in self.user_records.iter() {
            if let Some(client_connection) = self.client_connections.get_mut(&user_key) {
                //remove entity from user connection
                client_connection.despawn_entity(&self.world_record, entity);
            }
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

    /// Returns whether or not an Entity has an owner
    pub(crate) fn entity_has_owner(&self, entity: &E) -> bool {
        if let Some(record) = self.entity_records.get(entity) {
            return record.owner_key.is_some();
        }
        return false;
    }

    /// Gets the UserKey of the User that currently owns an Entity, if it exists
    pub(crate) fn entity_get_owner(&self, entity: &E) -> Option<UserKey> {
        if let Some(record) = self.entity_records.get(entity) {
            return record.owner_key;
        }
        return None;
    }

    /// Set the 'owner' of an Entity to a User associated with a given UserKey.
    /// Users are only able to issue Commands to Entities of which they are the
    /// owner
    pub(crate) fn entity_set_owner(&mut self, entity: &E, user_key: &UserKey) {
        // check that entity is initialized & un-owned
        if let Some(entity_record) = self.entity_records.get_mut(entity) {
            if entity_record.owner_key.is_some() {
                panic!("attempting to take ownership of an Entity that is already owned");
            };

            // get at the User's connection
            if let Some(client_connection) = self.client_connections.get_mut(user_key) {
                // add Entity to User's connection if it's not already in-scope
                if !client_connection.has_entity(entity) {
                    //add entity to user connection
                    client_connection.spawn_entity(&self.world_record, entity);
                }

                // assign Entity to User as a Prediction
                client_connection.add_prediction_entity(entity);
            }

            // put in ownership map
            entity_record.owner_key = Some(*user_key);
            if let Some(user) = self.user_records.get_mut(*user_key) {
                user.owned_entities.insert(*entity);
            }
        }
    }

    /// Removes ownership of an Entity from their current owner User.
    /// No User is able to issue Commands to an un-owned Entity.
    pub(crate) fn entity_disown(&mut self, entity: &E) {
        // a couple sanity checks ..
        if let Some(entity_record) = self.entity_records.get_mut(entity) {
            let current_owner_key: UserKey = entity_record
                .owner_key
                .expect("attempting to disown entity that does not have an owner..");

            if let Some(client_connection) = self.client_connections.get_mut(&current_owner_key) {
                client_connection.remove_prediction_entity(entity);
            }

            // remove from ownership map
            entity_record.owner_key = None;
            if let Some(user) = self.user_records.get_mut(current_owner_key) {
                user.owned_entities.remove(entity);
            }
        }
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
        for (user_key, _) in self.user_records.iter() {
            if let Some(client_connection) = self.client_connections.get_mut(&user_key) {
                if client_connection.has_entity(entity) {
                    // insert component into user's connection
                    client_connection.insert_component(&self.world_record, &component_key);
                }
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
        for (user_key, _) in self.user_records.iter() {
            if let Some(client_connection) = self.client_connections.get_mut(&user_key) {
                //remove component from user connection
                client_connection.remove_component(&component_key);
            }
        }

        // cleanup all other loose ends
        self.component_cleanup(&component_key);

        // remove from world
        return world.remove_component::<R>(entity);
    }

    //// Users

    /// Get a User's Socket Address, given the associated UserKey
    pub(crate) fn get_user_address(&self, user_key: &UserKey) -> Option<SocketAddr> {
        if let Some(user_record) = self.user_records.get(*user_key) {
            return Some(user_record.user.address);
        }
        return None;
    }

    pub(crate) fn user_force_disconnect(&mut self, user_key: &UserKey) {
        self.outstanding_disconnects.push_back(*user_key);
    }

    /// All necessary cleanup, when they're actually gone...
    pub(crate) fn delete_user(&mut self, user_key: &UserKey) -> Option<User> {
        // TODO: cache this?
        // Clean up all user data
        for (_, room) in self.rooms.iter_mut() {
            room.unsubscribe_user(&user_key);
        }

        if let Some(user_record) = self.user_records.remove(*user_key) {
            self.address_to_user_key_map.remove(&user_record.user.address);
            self.client_connections.remove(&user_key);
            self.entity_scope_map.remove_user(user_key);

            for owned_entity in user_record.owned_entities {
                if let Some(entity_record) = self.entity_records.get_mut(&owned_entity) {
                    entity_record.owner_key = None;
                }
            }

            return Some(user_record.user);
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

    // Private methods

    fn maintain_socket(&mut self) {
        // heartbeats
        if self.heartbeat_timer.ringing() {
            self.heartbeat_timer.reset();

            let server_tick_opt = self.server_tick();

            for (user_key, connection) in self.client_connections.iter_mut() {
                if let Some(user_record) = self.user_records.get(*user_key) {
                    if connection.should_drop() {
                        self.outstanding_disconnects.push_back(*user_key);
                    } else {
                        if connection.should_send_heartbeat() {
                            // Don't try to refactor this to self.internal_send, doesn't seem to
                            // work cause of iter_mut()
                            let payload = connection.process_outgoing_header(
                                server_tick_opt,
                                connection.get_last_received_tick(),
                                PacketType::Heartbeat,
                                &[],
                            );
                            self.io.send_packet(Packet::new_raw(user_record.user.address, payload));
                            connection.mark_sent();
                        }
                    }
                }
            }
        }

        //receive socket events
        loop {
            match self.io.receive_packet() {
                Ok(Some(packet)) => {
                    let address = packet.address();
                    if let Some(user_key) = self.address_to_user_key_map.get(&address) {
                        match self.client_connections.get_mut(&user_key) {
                            Some(connection) => {
                                connection.mark_heard();
                            }
                            None => {} //not yet established connection
                        }
                    }

                    let (header, payload) = StandardHeader::read(packet.payload());

                    match header.packet_type() {
                        PacketType::ClientChallengeRequest => {
                            let mut reader = PacketReader::new(&payload);
                            let timestamp = Timestamp::read(&mut reader);

                            let mut timestamp_bytes = Vec::new();
                            timestamp.write(&mut timestamp_bytes);
                            let timestamp_hash: hmac::Tag =
                                hmac::sign(&self.connection_hash_key, &timestamp_bytes);

                            let mut payload_bytes = Vec::new();
                            // write current tick
                            payload_bytes
                                .write_u16::<BigEndian>(self.server_tick().unwrap_or(0))
                                .unwrap();

                            //write timestamp
                            payload_bytes.append(&mut timestamp_bytes);

                            //write timestamp digest
                            let hash_bytes: &[u8] = timestamp_hash.as_ref();
                            for hash_byte in hash_bytes {
                                payload_bytes.push(*hash_byte);
                            }

                            // Send connectionless //
                            let packet = Packet::new(address, payload_bytes);
                            let new_payload = naia_shared::utils::write_connectionless_payload(
                                PacketType::ServerChallengeResponse,
                                packet.payload(),
                            );
                            self.io
                                .send_packet(Packet::new_raw(packet.address(), new_payload));
                            /////////////////////////
                        }
                        PacketType::ClientConnectRequest => {
                            let mut reader = PacketReader::new(&payload);
                            let timestamp = Timestamp::read(&mut reader);

                            if let Some(user_key) = self.address_to_user_key_map.get(&address) {
                                // At this point, we have already sent the ServerConnectResponse
                                // message, but we continue to send the message till the Client
                                // stops sending the ClientConnectRequest
                                if self.client_connections.contains_key(user_key) {
                                    let user = self.user_records.get(*user_key).unwrap();
                                    if user.timestamp == timestamp {
                                        let connection =
                                            self.client_connections.get_mut(user_key).unwrap();
                                        connection
                                            .process_incoming_header(&self.world_record, &header);

                                        // send connect accept message //
                                        let payload = connection.process_outgoing_header(
                                            None,
                                            0,
                                            PacketType::ServerConnectResponse,
                                            &[],
                                        );
                                        self.io.send_packet(Packet::new_raw(
                                            connection.get_address(),
                                            payload,
                                        ));
                                        connection.mark_sent();
                                        /////////////////////////////////
                                    } else {
                                        self.outstanding_disconnects.push_back(*user_key);
                                    }
                                } else {
                                    error!("if there's a user key associated with the address, should also have a client connection initiated");
                                }
                            } else {
                                // Verify that timestamp hash has been written by this
                                // server instance
                                let mut timestamp_bytes: Vec<u8> = Vec::new();
                                timestamp.write(&mut timestamp_bytes);
                                let mut digest_bytes: Vec<u8> = Vec::new();
                                for _ in 0..32 {
                                    digest_bytes.push(reader.read_u8());
                                }
                                let validation_result = hmac::verify(
                                    &self.connection_hash_key,
                                    &timestamp_bytes,
                                    &digest_bytes,
                                );
                                if validation_result.is_err() {
                                    continue;
                                }

                                // Timestamp hash is validated, now start configured auth process
                                let user = UserRecord::new(address, timestamp);
                                let user_key = self.user_records.insert(user);

                                let has_auth = reader.read_u8() == 1;

                                if has_auth != self.require_auth {
                                    self.reject_connection(&user_key);
                                    continue;
                                }

                                if has_auth {
                                    let auth_kind = P::Kind::from_u16(reader.read_u16());
                                    let auth_message =
                                        self.manifest.create_replica(auth_kind, &mut reader, 0);
                                    self.outstanding_auths.push_back((user_key, auth_message));
                                } else {
                                    self.accept_connection(&user_key);
                                }
                            }
                        }
                        PacketType::Data => {
                            if let Some(user_key) = self.address_to_user_key_map.get(&address) {
                                let server_tick_opt = self.server_tick();
                                match self.client_connections.get_mut(user_key) {
                                    Some(connection) => {
                                        connection
                                            .process_incoming_header(&self.world_record, &header);
                                        connection.process_incoming_data(
                                            server_tick_opt,
                                            header.host_tick(),
                                            &self.manifest,
                                            &payload,
                                        );
                                    }
                                    None => {
                                        warn!(
                                            "received data from unauthenticated client: {}",
                                            address
                                        );
                                    }
                                }
                            }
                        }
                        PacketType::Heartbeat => {
                            if let Some(user_key) = self.address_to_user_key_map.get(&address) {
                                match self.client_connections.get_mut(user_key) {
                                    Some(connection) => {
                                        // Still need to do this so that proper notify
                                        // events fire based on the heartbeat header
                                        connection
                                            .process_incoming_header(&self.world_record, &header);
                                    }
                                    None => {
                                        warn!(
                                            "received heartbeat from unauthenticated client: {}",
                                            address
                                        );
                                    }
                                }
                            }
                        }
                        PacketType::Ping => {
                            if let Some(user_key) = self.address_to_user_key_map.get(&address) {
                                let server_tick_opt = self.server_tick();
                                match self.client_connections.get_mut(user_key) {
                                    Some(connection) => {
                                        connection
                                            .process_incoming_header(&self.world_record, &header);
                                        let ping_payload = connection.process_ping(&payload);
                                        let payload_with_header = connection
                                            .process_outgoing_header(
                                                server_tick_opt,
                                                connection.get_last_received_tick(),
                                                PacketType::Pong,
                                                &ping_payload,
                                            );
                                        self.io.send_packet(Packet::new_raw(
                                            connection.get_address(),
                                            payload_with_header,
                                        ));
                                        connection.mark_sent();
                                    }
                                    None => {
                                        warn!(
                                            "received ping from unauthenticated client: {}",
                                            address
                                        );
                                    }
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
                if let Some(client_connection) = self.client_connections.get_mut(&removed_user) {
                    //remove entity from user connection
                    client_connection.despawn_entity(&self.world_record, &removed_entity);
                }
            }

            // TODO: we should be able to cache these tuples of keys to avoid building a new
            // list each time
            for user_key in room.user_keys() {
                for entity in room.entities() {
                    if world.has_entity(entity) {
                        if let Some(client_connection) = self.client_connections.get_mut(user_key) {
                            let currently_in_scope = client_connection.has_entity(entity);

                            let should_be_in_scope: bool;
                            if client_connection.has_prediction_entity(entity) {
                                should_be_in_scope = true;
                            } else {
                                if let Some(in_scope) = self.entity_scope_map.get(user_key, entity)
                                {
                                    should_be_in_scope = *in_scope;
                                } else {
                                    should_be_in_scope = false;
                                }
                            }

                            if should_be_in_scope {
                                if !currently_in_scope {
                                    // add entity to the connections local scope
                                    client_connection.spawn_entity(&self.world_record, entity);
                                }
                            } else {
                                if currently_in_scope {
                                    // remove entity from the connections local scope
                                    client_connection.despawn_entity(&self.world_record, entity);
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

// IO
struct Io {
    packet_sender: Option<PacketSender>,
    packet_receiver: Option<PacketReceiver>,
}

impl Io {
    pub fn new() -> Self {
        Io {
            packet_sender: None,
            packet_receiver: None,
        }
    }

    pub fn load(&mut self, packet_sender: PacketSender, packet_receiver: PacketReceiver) {
        if self.packet_sender.is_some() {
            panic!("Packet sender/receiver already loaded! Cannot do this twice!");
        }

        self.packet_sender = Some(packet_sender);
        self.packet_receiver = Some(packet_receiver);
    }

    pub fn send_packet(&self, packet: Packet) {
        self.packet_sender
            .as_ref()
            .expect("Cannot call Server.send_packet() until you call Server.listen()!")
            .send(packet);
    }

    pub fn receive_packet(&mut self) -> Result<Option<Packet>, NaiaServerSocketError> {
        return self
            .packet_receiver
            .as_mut()
            .expect("Cannot call Server.receive_packet() until you call Server.listen()!")
            .receive();
    }
}
