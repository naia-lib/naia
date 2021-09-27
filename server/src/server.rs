use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    panic,
};

use byteorder::{BigEndian, WriteBytesExt};
use ring::{hmac, rand};
use slotmap::DenseSlotMap;

use naia_server_socket::{
    NaiaServerSocketError, Packet, PacketReceiver, PacketSender, ServerAddrs, Socket,
};

pub use naia_shared::{
    wrapping_diff, Connection, ConnectionConfig, HostTickManager, ImplRef, Instant, KeyGenerator,
    LocalComponentKey, ManagerType, Manifest, PacketReader, PacketType, PropertyMutate,
    ProtocolType, Ref, Replicate, SharedConfig, StandardHeader, Timer, Timestamp,
};

use super::{
    client_connection::ClientConnection,
    error::NaiaServerError,
    event::Event,
    keys::{ComponentKey, KeyType},
    mut_handler::MutHandler,
    property_mutator::PropertyMutator,
    room::{room_key::RoomKey, Room, RoomMut, RoomRef},
    server_config::ServerConfig,
    tick_manager::TickManager,
    user::{user_key::UserKey, User, UserMut, UserRef},
    user_scope::UserScopeMut,
    world_ref::{WorldMut, WorldRef},
    world_type::WorldType,
};

/// A server that uses either UDP or WebRTC communication to send/receive
/// messages to/from connected clients, and syncs registered entities to
/// clients to whom they are in-scope
pub struct Server<P: ProtocolType, W: WorldType<P>> {
    // Config
    manifest: Manifest<P>,
    // Connection
    connection_config: ConnectionConfig,
    socket: Socket,
    io: Io,
    heartbeat_timer: Timer,
    connection_hash_key: hmac::Key,
    // Users
    users: DenseSlotMap<UserKey, User>,
    address_to_user_key_map: HashMap<SocketAddr, UserKey>,
    client_connections: HashMap<UserKey, ClientConnection<P, W>>,
    // Rooms
    rooms: DenseSlotMap<RoomKey, Room<P, W>>,
    // Entities
    entity_owner_map: HashMap<W::EntityKey, Option<UserKey>>,
    entity_room_map: HashMap<W::EntityKey, RoomKey>,
    entity_scope_map: HashMap<(UserKey, W::EntityKey), bool>,
    // Components
    mut_handler: Ref<MutHandler<W::EntityKey>>,
    // Events
    outstanding_connects: VecDeque<UserKey>,
    outstanding_disconnects: VecDeque<UserKey>,
    outstanding_auths: VecDeque<(UserKey, P)>,
    outstanding_errors: VecDeque<NaiaServerError>,
    // Ticks
    tick_manager: TickManager,
}

impl<P: ProtocolType, W: WorldType<P>> Server<P, W> {
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

        Server {
            // Config
            manifest: shared_config.manifest,
            // Connection
            connection_config,
            socket,
            io: Io::new(),
            heartbeat_timer,
            connection_hash_key,
            // Users
            users: DenseSlotMap::with_key(),
            address_to_user_key_map: HashMap::new(),
            client_connections: clients_map,
            // Rooms
            rooms: DenseSlotMap::with_key(),
            // Entities
            entity_owner_map: HashMap::new(),
            entity_room_map: HashMap::new(),
            entity_scope_map: HashMap::new(),
            // Components
            mut_handler: MutHandler::new(),
            // Events
            outstanding_auths: VecDeque::new(),
            outstanding_connects: VecDeque::new(),
            outstanding_disconnects: VecDeque::new(),
            outstanding_errors: VecDeque::new(),
            // Ticks
            tick_manager: TickManager::new(shared_config.tick_interval),
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
    pub(crate) fn receive(&mut self, world: &W) -> VecDeque<Result<Event<P, W>, NaiaServerError>> {
        let mut events = VecDeque::new();

        // Need to run this to maintain connection with all clients, and receive packets
        // until none left
        self.maintain_socket(world);

        // new authorizations
        while let Some((user_key, auth_message)) = self.outstanding_auths.pop_front() {
            events.push_back(Ok(Event::Authorization(user_key, auth_message)));
        }

        // new connections
        while let Some(user_key) = self.outstanding_connects.pop_front() {
            if let Some(user) = self.users.get(user_key) {
                self.address_to_user_key_map.insert(user.address, user_key);

                let mut new_connection = ClientConnection::new(
                    user.address,
                    Some(&self.mut_handler),
                    &self.connection_config,
                );
                //new_connection.process_incoming_header(&header);// not sure if I should
                // uncomment this...

                // send connect accept message //
                let payload = new_connection.process_outgoing_header(
                    0,
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
            // Clean up all user data
            for (_, room) in self.rooms.iter_mut() {
                room.unsubscribe_user(&user_key);
            }

            let user_clone = self.users.get(user_key).unwrap().clone();
            self.address_to_user_key_map.remove(&user_clone.address);
            self.users.remove(user_key);
            self.client_connections.remove(&user_key);

            events.push_back(Ok(Event::Disconnection(user_key, user_clone)));
        }

        // TODO: have 1 single queue for commands/messages from all users, as it's
        // possible this current technique unfairly favors the 1st users in
        // self.client_connections
        for (user_key, connection) in self.client_connections.iter_mut() {
            //receive commands from anyone
            while let Some((prediction_key, command)) =
                connection.get_incoming_command(self.tick_manager.get_tick())
            {
                events.push_back(Ok(Event::Command(*user_key, prediction_key, command)));
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
        if self.tick_manager.should_tick() {
            events.push_back(Ok(Event::Tick));
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
        self.users.remove(*user_key);
    }

    // Messages

    /// Queues up an Message to be sent to the Client associated with a given
    /// UserKey
    pub fn queue_message<R: ImplRef<P>>(
        &mut self,
        user_key: &UserKey,
        message_ref: &R,
        guaranteed_delivery: bool,
    ) {
        if let Some(connection) = self.client_connections.get_mut(user_key) {
            let dyn_ref = message_ref.dyn_ref();
            connection.queue_message(&dyn_ref, guaranteed_delivery);
        }
    }

    // Updates ... ?

    /// Sends all update messages to all Clients. If you don't call this
    /// method, the Server will never communicate with it's connected
    /// Clients
    pub(crate) fn send_all_updates(&mut self, world: &W) {
        // update entity scopes
        self.update_entity_scopes(world);

        // loop through all connections, send packet
        for (user_key, connection) in self.client_connections.iter_mut() {
            if let Some(user) = self.users.get(*user_key) {
                connection.collect_component_updates(world);
                while let Some(payload) = connection.get_outgoing_packet(
                    world,
                    self.tick_manager.get_tick(),
                    &self.manifest,
                ) {
                    self.io.send_packet(Packet::new_raw(user.address, payload));
                    connection.mark_sent();
                }
            }
        }
    }

    // World

    /// Retrieves a WorldRef that exposes read-only World operations
    pub fn world<'s, 'w>(&'s self, world: &'w W) -> WorldRef<'s, 'w, P, W> {
        return WorldRef::new(self, world);
    }

    /// Retrieves a WorldMut that exposes read and write World operations
    pub fn world_mut<'s, 'w>(&'s mut self, world: &'w mut W) -> WorldMut<'s, 'w, P, W> {
        return WorldMut::new(self, world);
    }

    // Entity Scopes

    /// Returns a UserScopeMut, which
    pub fn user_scope(&mut self, user_key: &UserKey) -> UserScopeMut<P, W> {
        if self.users.contains_key(*user_key) {
            return UserScopeMut::new(self, &user_key);
        }
        panic!("No User exists for given Key!");
    }

    /// Used to evaluate whether, given a User & Entity that are in the
    /// same Room, said Entity should be in scope for the given User.
    ///
    /// While Rooms allow for a very simple scope to which an Entity can belong,
    /// this provides complete customization for advanced scopes.
    ///
    /// Return a collection of Entity Scope Sets, being a unique combination of
    /// a related Room, User, and Entity, used to determine which Entities to
    /// replicate to which Users
    pub fn scope_checks(&self) -> Vec<(RoomKey, UserKey, W::EntityKey)> {
        let mut list: Vec<(RoomKey, UserKey, W::EntityKey)> = Vec::new();

        // TODO: precache this, instead of generating a new list every call
        // likely this is called A LOT
        for (room_key, room) in self.rooms.iter() {
            for user_key in room.user_keys() {
                for entity_key in room.entity_keys() {
                    list.push((room_key, *user_key, *entity_key));
                }
            }
        }

        return list;
    }

    // Rooms

    /// Creates a new Room on the Server and returns a corresponding RoomMut,
    /// which can be used to add users/entities to the room or retrieve its
    /// key
    pub fn make_room(&mut self) -> RoomMut<P, W> {
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
    pub fn room(&self, room_key: &RoomKey) -> RoomRef<P, W> {
        if self.rooms.contains_key(*room_key) {
            return RoomRef::new(self, room_key);
        }
        panic!("No Room exists for given Key!");
    }

    /// Retrieves an RoomMut that exposes read and write operations for the
    /// Room associated with the given RoomKey.
    /// Panics if the room does not exist.
    pub fn room_mut(&mut self, room_key: &RoomKey) -> RoomMut<P, W> {
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

    // Users

    /// Returns whether or not a User exists for the given RoomKey
    pub fn user_exists(&self, user_key: &UserKey) -> bool {
        return self.users.contains_key(*user_key);
    }

    /// Retrieves an UserRef that exposes read-only operations for the User
    /// associated with the given UserKey.
    /// Panics if the user does not exist.
    pub fn user(&self, user_key: &UserKey) -> UserRef<P, W> {
        if self.users.contains_key(*user_key) {
            return UserRef::new(self, &user_key);
        }
        panic!("No User exists for given Key!");
    }

    /// Retrieves an UserMut that exposes read and write operations for the User
    /// associated with the given UserKey.
    /// Returns None if the user does not exist.
    pub fn user_mut(&mut self, user_key: &UserKey) -> UserMut<P, W> {
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

    // User Scopes

    /// Returns true if a given User has an Entity with a given Entity Key
    /// in-scope currently
    pub fn user_scope_has_entity(&self, user_key: &UserKey, entity_key: &W::EntityKey) -> bool {
        if let Some(client_connection) = self.client_connections.get(user_key) {
            return client_connection.has_entity(entity_key);
        }

        return false;
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
    pub fn server_tick(&self) -> u16 {
        self.tick_manager.get_tick()
    }

    // Crate-Public methods

    //// Entities

    /// Despawns the Entity associated with the given Entity Key, if it exists.
    /// This will also remove all of the entity’s Components.
    /// Returns true if the entity is successfully despawned and false if the
    /// entity does not exist.
    pub(crate) fn despawn_entity(&mut self, world: &W, entity_key: &W::EntityKey) {
        if !world.has_entity(entity_key) {
            panic!("attempted to de-spawn nonexistent entity");
        }
        // Clean up ownership if applicable
        if self.entity_has_owner(entity_key) {
            self.entity_disown(entity_key);
        }

        // TODO: we can make this more efficient in the future by caching which Entities
        // are in each User's scope
        for (user_key, _) in self.users.iter() {
            if let Some(client_connection) = self.client_connections.get_mut(&user_key) {
                Self::connection_remove_entity(client_connection, world, entity_key);
            }
        }

        // Clean up associated components
        for component_protocol in world.get_components(entity_key) {
            let component_key = ComponentKey::new(entity_key, &component_protocol.get_type_id());
            self.component_cleanup(&component_key);
        }
    }

    /// Returns whether or not an Entity associated with the given Entity Key
    /// has an owner
    pub(crate) fn entity_has_owner(&self, entity_key: &W::EntityKey) -> bool {
        if let Some(owner_opt) = self.entity_owner_map.get(entity_key) {
            return owner_opt.is_some();
        }
        return false;
    }

    /// Gets the UserKey of the User that currently owns an Entity associated
    /// with the given Entity Key, if it exists
    pub(crate) fn entity_get_owner(&self, entity_key: &W::EntityKey) -> Option<&UserKey> {
        if let Some(owner_opt) = self.entity_owner_map.get(entity_key) {
            return owner_opt.as_ref();
        }
        return None;
    }

    /// Set the 'owner' of an Entity associated with a given Entity Key to a
    /// User associated with a given UserKey. Users are only able to issue
    /// Commands to Entities of which they are the owner
    pub(crate) fn entity_set_owner(
        &mut self,
        world: &W,
        entity_key: &W::EntityKey,
        user_key: &UserKey,
    ) {
        // check that entity is initialized & un-owned
        if self
            .entity_owner_map
            .get(entity_key)
            .expect("Entity/owner map must be uninitialized")
            .is_some()
        {
            panic!("attempting to take ownership of an Entity that is already owned");
        };

        // get at the User's connection
        let client_connection = self
            .client_connections
            .get_mut(user_key)
            .expect("User associated with given UserKey does not exist!");

        // add Entity to User's connection if it's not already in-scope
        if !client_connection.has_entity(entity_key) {
            Self::connection_add_entity(client_connection, world, entity_key);
        }

        // assign Entity to User as a Prediction
        client_connection.add_prediction_entity(entity_key);

        // put in ownership map
        self.entity_owner_map.insert(*entity_key, Some(*user_key));
    }

    /// Removes ownership of an Entity from their current owner User.
    /// No User is able to issue Commands to an un-owned Entity.
    pub(crate) fn entity_disown(&mut self, entity_key: &W::EntityKey) {
        // a couple sanity checks ..
        let current_owner_key: UserKey = self
            .entity_owner_map
            .get(entity_key)
            .expect("entity does not exist in owner map")
            .expect("attempting to disown entity that does not have an owner..");

        let client_connection = self
            .client_connections
            .get_mut(&current_owner_key)
            .expect("user which owns entity does not seem to have a connection still..");

        client_connection.remove_prediction_entity(entity_key);
        self.entity_owner_map.insert(*entity_key, None);
    }

    //// Entity Scopes

    pub(crate) fn user_scope_set_entity(
        &mut self,
        user_key: &UserKey,
        entity_key: &W::EntityKey,
        is_contained: bool,
    ) {
        let key = (*user_key, *entity_key);
        self.entity_scope_map.insert(key, is_contained);
    }

    //// Components

    /// Adds a Component to an Entity associated with the given Entity Key.
    pub(crate) fn insert_component<R: ImplRef<P>>(
        &mut self,
        world: &mut W,
        entity_key: &W::EntityKey,
        component_ref: &R,
    ) {
        if !world.has_entity(entity_key) {
            panic!("attempted to add component to non-existent entity");
        }

        let dyn_ref: Ref<dyn Replicate<P>> = component_ref.dyn_ref();
        let type_id = &dyn_ref.borrow().get_type_id();

        if world.has_component_dynamic(entity_key, &type_id) {
            panic!(
                "attempted to add component to entity which already has one of that type! \
                   an entity is not allowed to have more than 1 type of component at a time."
            )
        }

        world.insert_component(entity_key, component_ref.clone_ref());

        let component_key: ComponentKey<W::EntityKey> =
            self.component_init(entity_key, component_ref);

        // add component to connections already tracking entity
        for (user_key, _) in self.users.iter() {
            if let Some(client_connection) = self.client_connections.get_mut(&user_key) {
                if client_connection.has_entity(entity_key) {
                    Self::connection_insert_component(client_connection, world, &component_key);
                }
            }
        }
    }

    /// Removes a Component from an Entity associated with the given Entity Key
    pub(crate) fn remove_component<R: Replicate<P>>(
        &mut self,
        world: &mut W,
        entity_key: &W::EntityKey,
    ) -> Option<Ref<R>> {
        // get at record
        if let Some(component_ref) = world.get_component::<R>(entity_key) {
            // get component key from type
            let component_key =
                ComponentKey::new(entity_key, &component_ref.borrow().get_type_id());

            // clean up component on all connections
            // TODO: should be able to make this more efficient by caching for every Entity
            // which scopes they are part of
            for (user_key, _) in self.users.iter() {
                if let Some(client_connection) = self.client_connections.get_mut(&user_key) {
                    Self::connection_remove_component(client_connection, &component_key);
                }
            }

            // cleanup all other loose ends
            self.component_cleanup(&component_key);

            return Some(component_ref);
        }
        return None;
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

    //// Rooms

    /// Deletes the Room associated with a given RoomKey on the Server.
    /// Returns true if the Room existed.
    pub(crate) fn room_destroy(&mut self, room_key: &RoomKey) -> bool {
        if self.rooms.contains_key(*room_key) {
            // remove all entities from the entity_room_map
            for entity_key in self.rooms.get(*room_key).unwrap().entity_keys() {
                self.entity_room_map.remove(entity_key);
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
    pub(crate) fn room_has_entity(&self, room_key: &RoomKey, entity_key: &W::EntityKey) -> bool {
        if let Some(actual_room_key) = self.entity_room_map.get(entity_key) {
            return room_key == actual_room_key;
        }
        return false;
    }

    /// Add an Entity to a Room, given the appropriate RoomKey & Entity Key.
    /// Entities will only ever be in-scope for Users which are in a Room with
    /// them.
    pub(crate) fn room_add_entity(&mut self, room_key: &RoomKey, entity_key: &W::EntityKey) {
        if self.entity_room_map.contains_key(entity_key) {
            panic!("Entity already belongs to a Room! Remove the Entity from the Room before adding it to a new Room.");
        }

        if let Some(room) = self.rooms.get_mut(*room_key) {
            room.add_entity(entity_key);
            self.entity_room_map.insert(*entity_key, *room_key);
        }
    }

    /// Remove an Entity from a Room, given the appropriate RoomKey & Entity Key
    pub(crate) fn room_remove_entity(&mut self, room_key: &RoomKey, entity_key: &W::EntityKey) {
        if let Some(room) = self.rooms.get_mut(*room_key) {
            if room.remove_entity(entity_key) {
                self.entity_room_map.remove(entity_key);
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

    fn maintain_socket(&mut self, world: &W) {
        // heartbeats
        if self.heartbeat_timer.ringing() {
            self.heartbeat_timer.reset();

            for (user_key, connection) in self.client_connections.iter_mut() {
                if let Some(user) = self.users.get(*user_key) {
                    if connection.should_drop() {
                        self.outstanding_disconnects.push_back(*user_key);
                    } else {
                        if connection.should_send_heartbeat() {
                            // Don't try to refactor this to self.internal_send, doesn't seem to
                            // work cause of iter_mut()
                            let payload = connection.process_outgoing_header(
                                self.tick_manager.get_tick(),
                                connection.get_last_received_tick(),
                                PacketType::Heartbeat,
                                &[],
                            );
                            self.io.send_packet(Packet::new_raw(user.address, payload));
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
                                .write_u16::<BigEndian>(self.tick_manager.get_tick())
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
                                // message, but we continue to send
                                // the message till the Client stops sending
                                // the ClientConnectRequest
                                if self.client_connections.contains_key(user_key) {
                                    let user = self.users.get(*user_key).unwrap();
                                    if user.timestamp == timestamp {
                                        let connection =
                                            self.client_connections.get_mut(user_key).unwrap();
                                        connection.process_incoming_header(world, &header);

                                        // send connect accept message //
                                        let payload = connection.process_outgoing_header(
                                            0,
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
                                //Verify that timestamp hash has been written by this
                                // server instance
                                let mut timestamp_bytes: Vec<u8> = Vec::new();
                                timestamp.write(&mut timestamp_bytes);
                                let mut digest_bytes: Vec<u8> = Vec::new();
                                for _ in 0..32 {
                                    digest_bytes.push(reader.read_u8());
                                }
                                if hmac::verify(
                                    &self.connection_hash_key,
                                    &timestamp_bytes,
                                    &digest_bytes,
                                )
                                .is_ok()
                                {
                                    let user = User::new(address, timestamp);
                                    let user_key = self.users.insert(user);

                                    // Return authorization event
                                    let naia_id = reader.read_u16();

                                    let auth_message =
                                        self.manifest.create_replica(naia_id, &mut reader);

                                    self.outstanding_auths.push_back((user_key, auth_message));
                                }
                            }
                        }
                        PacketType::Data => {
                            if let Some(user_key) = self.address_to_user_key_map.get(&address) {
                                match self.client_connections.get_mut(user_key) {
                                    Some(connection) => {
                                        connection.process_incoming_header(world, &header);
                                        connection.process_incoming_data(
                                            self.tick_manager.get_tick(),
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
                                        connection.process_incoming_header(world, &header);
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
                                match self.client_connections.get_mut(user_key) {
                                    Some(connection) => {
                                        connection.process_incoming_header(world, &header);
                                        let ping_payload = connection.process_ping(&payload);
                                        let payload_with_header = connection
                                            .process_outgoing_header(
                                                self.tick_manager.get_tick(),
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

    // Entity Scopes

    fn update_entity_scopes(&mut self, world: &W) {
        for (_, room) in self.rooms.iter_mut() {
            while let Some((removed_user, removed_entity)) = room.pop_entity_removal_queue() {
                if let Some(client_connection) = self.client_connections.get_mut(&removed_user) {
                    Self::connection_remove_entity(client_connection, world, &removed_entity);
                }
            }

            // TODO: we should be able to cache these tuples of keys to avoid building a new
            // list each time
            for user_key in room.user_keys() {
                for entity_key in room.entity_keys() {
                    if world.has_entity(entity_key) {
                        if let Some(client_connection) = self.client_connections.get_mut(user_key) {
                            let currently_in_scope = client_connection.has_entity(entity_key);

                            let should_be_in_scope: bool;
                            if client_connection.has_prediction_entity(entity_key) {
                                should_be_in_scope = true;
                            } else {
                                let key = (*user_key, *entity_key);
                                if let Some(in_scope) = self.entity_scope_map.get(&key) {
                                    should_be_in_scope = *in_scope;
                                } else {
                                    should_be_in_scope = false;
                                }
                            }

                            if should_be_in_scope {
                                if !currently_in_scope {
                                    // add entity to the connections local scope
                                    Self::connection_add_entity(
                                        client_connection,
                                        world,
                                        entity_key,
                                    );
                                }
                            } else {
                                if currently_in_scope {
                                    // remove entity from the connections local scope
                                    Self::connection_remove_entity(
                                        client_connection,
                                        world,
                                        entity_key,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Component Helpers

    fn component_init<R: ImplRef<P>>(
        &mut self,
        entity_key: &W::EntityKey,
        component_ref: &R,
    ) -> ComponentKey<W::EntityKey> {
        let dyn_ref = component_ref.dyn_ref();
        let new_mutator_ref: Ref<PropertyMutator<W::EntityKey>> =
            Ref::new(PropertyMutator::new(&self.mut_handler));

        dyn_ref
            .borrow_mut()
            .set_mutator(&to_property_mutator(new_mutator_ref.clone()));

        let component_key = ComponentKey::new(entity_key, &dyn_ref.borrow().get_type_id());
        new_mutator_ref
            .borrow_mut()
            .set_component_key(component_key);
        self.mut_handler
            .borrow_mut()
            .register_component(&component_key);
        return component_key;
    }

    fn component_cleanup(&mut self, component_key: &ComponentKey<W::EntityKey>) {
        self.mut_handler
            .borrow_mut()
            .deregister_component(component_key);
    }

    // Scope helper operations

    fn connection_add_entity(
        client_connection: &mut ClientConnection<P, W>,
        world: &W,
        entity_key: &W::EntityKey,
    ) {
        //add entity to user connection
        client_connection.add_entity(world, entity_key);
    }

    fn connection_remove_entity(
        client_connection: &mut ClientConnection<P, W>,
        world: &W,
        entity_key: &W::EntityKey,
    ) {
        //remove entity from user connection
        client_connection.remove_entity(world, entity_key);
    }

    fn connection_insert_component(
        client_connection: &mut ClientConnection<P, W>,
        world: &W,
        component_key: &ComponentKey<W::EntityKey>,
    ) {
        //add component to user connection
        client_connection.insert_component(world, component_key);
    }

    fn connection_remove_component(
        client_connection: &mut ClientConnection<P, W>,
        component_key: &ComponentKey<W::EntityKey>,
    ) {
        //remove component from user connection
        client_connection.remove_component(component_key);
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

cfg_if! {
    if #[cfg(feature = "multithread")] {
        use std::sync::{Arc, Mutex};
        fn to_property_mutator_raw<K: 'static + KeyType>(eref: Arc<Mutex<PropertyMutator<K>>>) -> Arc<Mutex<dyn PropertyMutate>> {
            eref.clone()
        }
    } else {
        use std::{cell::RefCell, rc::Rc};
        fn to_property_mutator_raw<K: 'static + KeyType>(eref: Rc<RefCell<PropertyMutator<K>>>) -> Rc<RefCell<dyn PropertyMutate>> {
            eref.clone()
        }
    }
}

fn to_property_mutator<K: 'static + KeyType>(
    eref: Ref<PropertyMutator<K>>,
) -> Ref<dyn PropertyMutate> {
    let upcast_ref = to_property_mutator_raw(eref.inner());
    Ref::new_raw(upcast_ref)
}
