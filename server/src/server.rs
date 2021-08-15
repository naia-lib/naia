use std::{
    collections::{HashMap, HashSet, VecDeque},
    net::SocketAddr,
    panic,
    rc::Rc,
};

use byteorder::{BigEndian, WriteBytesExt};
use futures_util::{pin_mut, select, FutureExt, StreamExt};
use log::info;
use ring::{hmac, rand};
use slotmap::DenseSlotMap;

use naia_server_socket::{
    MessageSender, NaiaServerSocketError, Packet, ServerSocket, ServerSocketTrait,
};
pub use naia_shared::{
    wrapping_diff, Connection, ConnectionConfig, EntityKey, HostTickManager, Instant, KeyGenerator,
    LocalReplicaKey, ManagerType, Manifest, PacketReader, PacketType, PropertyMutate, ProtocolType,
    Ref, Replicate, SharedConfig, StandardHeader, Timer, Timestamp,
};

use super::{
    client_connection::ClientConnection,
    error::NaiaServerError,
    event::Event,
    interval::Interval,
    keys::{replica_key::ReplicaKey, ObjectKey},
    mut_handler::MutHandler,
    property_mutator::PropertyMutator,
    room::{room_key::RoomKey, Room},
    server_config::ServerConfig,
    tick_manager::TickManager,
    user::{user_key::UserKey, User},
};
use crate::{ComponentKey, GlobalPawnKey};

/// A server that uses either UDP or WebRTC communication to send/receive
/// messages to/from connected clients, and syncs registered objects/entities to
/// clients to whom they are in-scope
pub struct Server<T: ProtocolType> {
    connection_config: ConnectionConfig,
    manifest: Manifest<T>,
    socket: Box<dyn ServerSocketTrait>,
    sender: MessageSender,
    global_replica_store: DenseSlotMap<ReplicaKey, T>,
    global_object_set: HashSet<ObjectKey>,
    auth_func: Option<Rc<Box<dyn Fn(&UserKey, &T) -> bool>>>,
    mut_handler: Ref<MutHandler>,
    users: DenseSlotMap<UserKey, User>,
    rooms: DenseSlotMap<RoomKey, Room>,
    address_to_user_key_map: HashMap<SocketAddr, UserKey>,
    client_connections: HashMap<UserKey, ClientConnection<T>>,
    outstanding_disconnects: VecDeque<UserKey>,
    heartbeat_timer: Timer,
    connection_hash_key: hmac::Key,
    tick_manager: TickManager,
    tick_timer: Interval,
    object_scope_map: HashMap<(RoomKey, UserKey, ObjectKey), bool>,
    entity_scope_map: HashMap<(RoomKey, UserKey, EntityKey), bool>,
    entity_key_generator: KeyGenerator<EntityKey>,
    entity_component_map: HashMap<EntityKey, Ref<HashSet<ComponentKey>>>,
    component_entity_map: HashMap<ComponentKey, EntityKey>,
}

impl<U: ProtocolType> Server<U> {
    /// Create a new Server, given an address to listen at, a Replica
    /// manifest, and an optional Config
    pub async fn new(
        manifest: Manifest<U>,
        server_config: Option<ServerConfig>,
        shared_config: SharedConfig,
    ) -> Self {
        let server_config = match server_config {
            Some(config) => config,
            None => ServerConfig::default(),
        };

        let connection_config = ConnectionConfig::new(
            server_config.disconnection_timeout_duration,
            server_config.heartbeat_interval,
            server_config.ping_interval,
            server_config.rtt_sample_size,
        );

        let mut server_socket = ServerSocket::listen(
            server_config.socket_addresses.session_listen_addr,
            server_config.socket_addresses.webrtc_listen_addr,
            server_config.socket_addresses.public_webrtc_addr,
        )
        .await;
        if let Some(config) = &shared_config.link_condition_config {
            server_socket = server_socket.with_link_conditioner(config);
        }

        let sender = server_socket.get_sender();
        let clients_map = HashMap::new();
        let heartbeat_timer = Timer::new(connection_config.heartbeat_interval);

        let connection_hash_key =
            hmac::Key::generate(hmac::HMAC_SHA256, &rand::SystemRandom::new()).unwrap();

        Server {
            manifest,
            global_replica_store: DenseSlotMap::with_key(),
            global_object_set: HashSet::new(),
            object_scope_map: HashMap::new(),
            entity_scope_map: HashMap::new(),
            auth_func: None,
            mut_handler: MutHandler::new(),
            socket: server_socket,
            sender,
            connection_config,
            users: DenseSlotMap::with_key(),
            rooms: DenseSlotMap::with_key(),
            connection_hash_key,
            client_connections: clients_map,
            address_to_user_key_map: HashMap::new(),
            outstanding_disconnects: VecDeque::new(),
            heartbeat_timer,
            tick_manager: TickManager::new(shared_config.tick_interval),
            tick_timer: Interval::new(shared_config.tick_interval),
            entity_key_generator: KeyGenerator::new(),
            entity_component_map: HashMap::new(),
            component_entity_map: HashMap::new(),
        }
    }

    /// Must be called regularly, maintains connection to and receives messages
    /// from all Clients
    pub async fn receive(&mut self) -> Result<Event<U>, NaiaServerError> {
        loop {
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
                                self.sender
                                    .send(Packet::new_raw(user.address, payload))
                                    .await
                                    .expect("send failed!");
                                connection.mark_sent();
                            }
                        }
                    }
                }
            }

            // timeouts
            if let Some(user_key) = self.outstanding_disconnects.pop_front() {
                for (_, room) in self.rooms.iter_mut() {
                    room.unsubscribe_user(&user_key);
                }

                let address = self.users.get(user_key).unwrap().address;
                self.address_to_user_key_map.remove(&address);
                let user_clone = self.users.get(user_key).unwrap().clone();
                self.users.remove(user_key);
                self.client_connections.remove(&user_key);

                return Ok(Event::Disconnection(user_key, user_clone));
            }

            // TODO: have 1 single queue for commands/messages from all users, as it's
            // possible this current technique unfairly favors the 1st users in
            // self.client_connections
            for (user_key, connection) in self.client_connections.iter_mut() {
                //receive commands from anyone
                if let Some((pawn_key, command)) =
                    connection.get_incoming_command(self.tick_manager.get_tick())
                {
                    match pawn_key {
                        GlobalPawnKey::Object(object_key) => {
                            return Ok(Event::Command(*user_key, object_key, command));
                        }
                        GlobalPawnKey::Entity(entity_key) => {
                            return Ok(Event::CommandEntity(*user_key, entity_key, command));
                        }
                    }
                }
                //receive messages from anyone
                if let Some(message) = connection.get_incoming_message() {
                    return Ok(Event::Message(*user_key, message));
                }
            }

            //receive socket events
            enum Next {
                SocketResult(Result<Packet, NaiaServerSocketError>),
                Tick,
            }

            let next = {
                let timer_next = self.tick_timer.next().fuse();
                pin_mut!(timer_next);

                let socket_next = self.socket.receive().fuse();
                pin_mut!(socket_next);

                select! {
                    socket_result = socket_next => {
                        Next::SocketResult(socket_result)
                    }
                    _ = timer_next => {
                        Next::Tick
                    }
                }
            };

            match next {
                Next::SocketResult(result) => {
                    match result {
                        Ok(packet) => {
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

                                    Server::<U>::internal_send_connectionless(
                                        &mut self.sender,
                                        PacketType::ServerChallengeResponse,
                                        Packet::new(address, payload_bytes),
                                    )
                                    .await;

                                    continue;
                                }
                                PacketType::ClientConnectRequest => {
                                    let mut reader = PacketReader::new(&payload);
                                    let timestamp = Timestamp::read(&mut reader);

                                    if let Some(user_key) =
                                        self.address_to_user_key_map.get(&address)
                                    {
                                        if self.client_connections.contains_key(user_key) {
                                            let user = self.users.get(*user_key).unwrap();
                                            if user.timestamp == timestamp {
                                                let mut connection = self
                                                    .client_connections
                                                    .get_mut(user_key)
                                                    .unwrap();
                                                connection.process_incoming_header(&header);
                                                Server::<U>::send_connect_accept_message(
                                                    &mut connection,
                                                    &mut self.sender,
                                                )
                                                .await;
                                                continue;
                                            } else {
                                                self.outstanding_disconnects.push_back(*user_key);
                                                continue;
                                            }
                                        } else {
                                            error!("if there's a user key associated with the address, should also have a client connection initiated");
                                            continue;
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
                                        if !hmac::verify(
                                            &self.connection_hash_key,
                                            &timestamp_bytes,
                                            &digest_bytes,
                                        )
                                        .is_ok()
                                        {
                                            continue;
                                        }

                                        let user = User::new(address, timestamp);
                                        let user_key = self.users.insert(user);

                                        // Call auth function if there is one
                                        if let Some(auth_func) = &self.auth_func {
                                            let naia_id = reader.read_u16();

                                            let auth_message =
                                                self.manifest.create_replica(naia_id, &mut reader);
                                            if !(auth_func.as_ref().as_ref())(
                                                &user_key,
                                                &auth_message,
                                            ) {
                                                self.users.remove(user_key);
                                                continue;
                                            }
                                        }

                                        self.address_to_user_key_map.insert(address, user_key);

                                        // Success! Create new connection
                                        let mut new_connection = ClientConnection::new(
                                            address,
                                            Some(&self.mut_handler),
                                            &self.connection_config,
                                        );
                                        new_connection.process_incoming_header(&header);
                                        Server::<U>::send_connect_accept_message(
                                            &mut new_connection,
                                            &mut self.sender,
                                        )
                                        .await;
                                        self.client_connections.insert(user_key, new_connection);
                                        return Ok(Event::Connection(user_key));
                                    }
                                }
                                PacketType::Data => {
                                    if let Some(user_key) =
                                        self.address_to_user_key_map.get(&address)
                                    {
                                        match self.client_connections.get_mut(user_key) {
                                            Some(connection) => {
                                                connection.process_incoming_header(&header);
                                                connection.process_incoming_data(
                                                    self.tick_manager.get_tick(),
                                                    header.host_tick(),
                                                    &self.manifest,
                                                    &payload,
                                                );
                                                continue;
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
                                    if let Some(user_key) =
                                        self.address_to_user_key_map.get(&address)
                                    {
                                        match self.client_connections.get_mut(user_key) {
                                            Some(connection) => {
                                                // Still need to do this so that proper notify
                                                // events fire based on the heartbeat header
                                                connection.process_incoming_header(&header);
                                                continue;
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
                                    if let Some(user_key) =
                                        self.address_to_user_key_map.get(&address)
                                    {
                                        match self.client_connections.get_mut(user_key) {
                                            Some(connection) => {
                                                connection.process_incoming_header(&header);
                                                let ping_payload =
                                                    connection.process_ping(&payload);
                                                let payload_with_header = connection
                                                    .process_outgoing_header(
                                                        self.tick_manager.get_tick(),
                                                        connection.get_last_received_tick(),
                                                        PacketType::Pong,
                                                        &ping_payload,
                                                    );
                                                self.sender
                                                    .send(Packet::new_raw(
                                                        connection.get_address(),
                                                        payload_with_header,
                                                    ))
                                                    .await
                                                    .expect("send failed!");
                                                connection.mark_sent();
                                                continue;
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
                                _ => {}
                            }
                        }
                        Err(error) => {
                            //TODO: Determine if disconnecting a user based on a send error is the
                            // right thing to do
                            //
                            // if let
                            // NaiaServerSocketError::SendError(address) = error {
                            //                        if let Some(user_key) =
                            // self.address_to_user_key_map.get(&address).copied() {
                            //                            self.client_connections.remove(&user_key);
                            //                            output =
                            // Some(Ok(Event::Disconnection(user_key)));
                            //                            continue;
                            //                        }
                            //                    }

                            return Err(NaiaServerError::Wrapped(Box::new(error)));
                        }
                    }
                }
                Next::Tick => {
                    self.tick_manager.increment_tick();
                    return Ok(Event::Tick);
                }
            }
        }
    }

    /// Queues up an Message to be sent to the Client associated with a given
    /// UserKey
    pub fn queue_message(
        &mut self,
        user_key: &UserKey,
        message: &impl Replicate<U>,
        guaranteed_delivery: bool,
    ) {
        if let Some(connection) = self.client_connections.get_mut(user_key) {
            connection.queue_message(message, guaranteed_delivery);
        }
    }

    /// Sends all update messages to all Clients. If you don't call this
    /// method, the Server will never communicate with it's connected
    /// Clients
    pub async fn send_all_updates(&mut self) {
        // update object scopes
        self.update_object_scopes();

        // update entity scopes
        self.update_entity_scopes();

        // loop through all connections, send packet
        for (user_key, connection) in self.client_connections.iter_mut() {
            if let Some(user) = self.users.get(*user_key) {
                connection.collect_replica_updates();
                while let Some(payload) =
                    connection.get_outgoing_packet(self.tick_manager.get_tick(), &self.manifest)
                {
                    match self
                        .sender
                        .send(Packet::new_raw(user.address, payload))
                        .await
                    {
                        Ok(_) => {}
                        Err(err) => {
                            info!("send error! {}", err);
                        }
                    }
                    connection.mark_sent();
                }
            }
        }
    }

    /// Register an Object with the Server, whereby the Server will sync
    /// replicas of the Object to all connected Clients for which the Object
    /// is in scope. Gives back an ObjectKey which can be used to get the
    /// reference to the Object from the Server once again
    pub fn register_object(&mut self, object: U) -> ObjectKey {
        let new_mutator_ref: Ref<PropertyMutator> =
            Ref::new(PropertyMutator::new(&self.mut_handler));
        object
            .inner_ref()
            .borrow_mut()
            .set_mutator(&to_property_mutator(&new_mutator_ref));
        let object_key = self.global_replica_store.insert(object);
        self.global_object_set.insert(object_key);
        new_mutator_ref.borrow_mut().set_object_key(object_key);
        self.mut_handler.borrow_mut().register_replica(&object_key);
        return object_key;
    }

    /// Deregisters an Object with the Server, deleting local replicas of the
    /// Object on each Client
    pub fn deregister_object(&mut self, key: ObjectKey) -> U {
        if !self.global_object_set.contains(&key) {
            panic!("attempted to deregister an Object which was never registered");
        }

        for (user_key, _) in self.users.iter() {
            if let Some(user_connection) = self.client_connections.get_mut(&user_key) {
                Self::user_remove_object(user_connection, &key);
            }
        }

        self.mut_handler.borrow_mut().deregister_replica(&key);
        self.global_object_set.remove(&key);
        return self
            .global_replica_store
            .remove(key)
            .expect("replica not initialized correctly?");
    }

    /// Assigns an Object to a specific User, making it a Pawn for that User
    /// (meaning that the User will be able to issue Commands to that Pawn)
    pub fn assign_pawn(&mut self, user_key: &UserKey, object_key: &ObjectKey) {
        if let Some(object) = self.global_replica_store.get(*object_key) {
            if let Some(user_connection) = self.client_connections.get_mut(user_key) {
                Self::user_add_object(user_connection, object_key, &object);
            }
        }

        if self.global_object_set.contains(object_key) {
            if let Some(user_connection) = self.client_connections.get_mut(user_key) {
                user_connection.add_pawn(object_key);
            }
        }
    }

    /// Unassigns a Pawn from a specific User (meaning that the User will be
    /// unable to issue Commands to that Pawn)
    pub fn unassign_pawn(&mut self, user_key: &UserKey, object_key: &ObjectKey) {
        if let Some(user_connection) = self.client_connections.get_mut(user_key) {
            user_connection.remove_pawn(object_key);
        }
    }

    /// Register an Entity with the Server, whereby the Server will sync the
    /// replica of all the given Entity's Components to all connected Clients
    /// for which the Entity is in scope. Gives back an EntityKey which can
    /// be used to get the reference to the Entity from the Server once
    /// again
    pub fn register_entity(&mut self) -> EntityKey {
        let entity_key: EntityKey = self.entity_key_generator.generate();
        self.entity_component_map
            .insert(entity_key, Ref::new(HashSet::new()));
        return entity_key;
    }

    /// Deregisters an Entity with the Server, deleting local copies of the
    /// Entity on each Client
    pub fn deregister_entity(&mut self, key: &EntityKey) {
        for (user_key, _) in self.users.iter() {
            if let Some(user_connection) = self.client_connections.get_mut(&user_key) {
                Self::user_remove_entity(user_connection, key);
            }
        }

        self.entity_component_map.remove(key);
    }

    /// Assigns an Entity to a specific User, making it a Pawn for that User
    /// (meaning that the User will be able to issue Commands to that Pawn)
    pub fn assign_pawn_entity(&mut self, user_key: &UserKey, entity_key: &EntityKey) {
        if self.entity_component_map.contains_key(entity_key) {
            if let Some(user_connection) = self.client_connections.get_mut(user_key) {
                user_connection.add_pawn_entity(entity_key);
            }
        }
    }

    /// Unassigns a Pawn from a specific User (meaning that the User will be
    /// unable to issue Commands to that Pawn)
    pub fn unassign_pawn_entity(&mut self, user_key: &UserKey, entity_key: &EntityKey) {
        if self.entity_component_map.contains_key(entity_key) {
            if let Some(user_connection) = self.client_connections.get_mut(user_key) {
                user_connection.remove_pawn_entity(entity_key);
            }
        }
    }

    /// Register a Component with the Server, whereby the Server will sync the
    /// replica of the Component to all connected Clients for which the
    /// Component's Entity is in Scope.
    /// Gives back a ComponentKey which can be used to get the reference to the
    /// Component from the Server once again
    pub fn add_component_to_entity(
        &mut self,
        entity_key: &EntityKey,
        component: U,
    ) -> ComponentKey {
        if !self.entity_component_map.contains_key(&entity_key) {
            panic!("attempted to add component to non-existent entity");
        }

        let component_ref = component.inner_ref().clone();
        let component_key: ComponentKey = self.register_object(component);

        // add component to connections already tracking entity
        for (user_key, _) in self.users.iter() {
            if let Some(user_connection) = self.client_connections.get_mut(&user_key) {
                if user_connection.has_entity(entity_key) {
                    Self::user_add_component(
                        user_connection,
                        entity_key,
                        &component_key,
                        &component_ref,
                    );
                }
            }
        }

        self.component_entity_map.insert(component_key, *entity_key);

        if let Some(component_set_ref) = self.entity_component_map.get_mut(&entity_key) {
            component_set_ref.borrow_mut().insert(component_key);
        }

        return component_key;
    }

    /// Deregisters a Component with the Server, deleting local copies of the
    /// Component on each Client
    pub fn remove_component(&mut self, component_key: &ComponentKey) -> U {
        let entity_key = self
            .component_entity_map
            .remove(component_key)
            .expect("attempting to remove a component which does not exist");
        let mut component_set = self
            .entity_component_map
            .get_mut(&entity_key)
            .expect("component error during initialization causing issue with removal of component")
            .borrow_mut();
        for (user_key, _) in self.users.iter() {
            if let Some(user_connection) = self.client_connections.get_mut(&user_key) {
                Self::user_remove_object(user_connection, component_key);
            }
        }

        component_set.remove(component_key);

        self.mut_handler
            .borrow_mut()
            .deregister_replica(component_key);
        return self
            .global_replica_store
            .remove(*component_key)
            .expect("component not initialized correctly?");
    }

    /// Given an ObjectKey, get a reference to a registered Object being tracked
    /// by the Server
    pub fn get_object(&mut self, key: ObjectKey) -> Option<&U> {
        if self.global_object_set.contains(&key) {
            return self.global_replica_store.get(key);
        } else {
            return None;
        }
    }

    /// Iterate through all the Server's Objects
    pub fn objects_iter(&self) -> std::collections::hash_set::Iter<ObjectKey> {
        return self.global_object_set.iter();
    }

    /// Get the number of Objects tracked by the Server
    pub fn get_objects_count(&self) -> usize {
        return self.global_object_set.len();
    }

    /// Creates a new Room on the Server, returns a Key which can be used to
    /// reference said Room
    pub fn create_room(&mut self) -> RoomKey {
        let new_room = Room::new();
        return self.rooms.insert(new_room);
    }

    /// Deletes the Room associated with a given RoomKey on the Server
    pub fn delete_room(&mut self, key: RoomKey) {
        self.rooms.remove(key);
    }

    /// Gets a Room given an associated RoomKey
    pub fn get_room(&self, key: RoomKey) -> Option<&Room> {
        return self.rooms.get(key);
    }

    /// Gets a mutable Room given an associated RoomKey
    pub fn get_room_mut(&mut self, key: RoomKey) -> Option<&mut Room> {
        return self.rooms.get_mut(key);
    }

    /// Iterate through all the Server's current Rooms
    pub fn rooms_iter(&self) -> slotmap::dense::Iter<RoomKey, Room> {
        return self.rooms.iter();
    }

    /// Get the number of Rooms in the Server
    pub fn get_rooms_count(&self) -> usize {
        return self.rooms.len();
    }

    /// Add an Object to a Room, given the appropriate RoomKey & ObjectKey
    /// Objects will only ever be in-scope for Users which are in a Room with
    /// them
    pub fn room_add_object(&mut self, room_key: &RoomKey, object_key: &ObjectKey) {
        if self.global_object_set.contains(object_key) {
            if let Some(room) = self.rooms.get_mut(*room_key) {
                room.add_object(object_key);
            }
        }
    }

    /// Remove an Object from a Room, given the appropriate RoomKey & ObjectKey
    pub fn room_remove_object(&mut self, room_key: &RoomKey, object_key: &ObjectKey) {
        if let Some(room) = self.rooms.get_mut(*room_key) {
            room.remove_object(object_key);
        }
    }

    /// Add an User to a Room, given the appropriate RoomKey & UserKey
    /// Objects/Entities will only ever be in-scope for Users which are in a
    /// Room with them
    pub fn room_add_user(&mut self, room_key: &RoomKey, user_key: &UserKey) {
        if let Some(room) = self.rooms.get_mut(*room_key) {
            room.subscribe_user(user_key);
        }
    }

    /// Removes a User from a Room
    pub fn room_remove_user(&mut self, room_key: &RoomKey, user_key: &UserKey) {
        if let Some(room) = self.rooms.get_mut(*room_key) {
            room.unsubscribe_user(user_key);
        }
    }

    /// Add an Entity to a Room, given the appropriate RoomKey & EntityKey
    /// Entities will only ever be in-scope for Users which are in a Room with
    /// them
    pub fn room_add_entity(&mut self, room_key: &RoomKey, entity_key: &EntityKey) {
        if let Some(room) = self.rooms.get_mut(*room_key) {
            room.add_entity(entity_key);
        }
    }

    /// Remove an Entity from a Room, given the appropriate RoomKey & EntityKey
    pub fn room_remove_entity(&mut self, room_key: &RoomKey, entity_key: &EntityKey) {
        if let Some(room) = self.rooms.get_mut(*room_key) {
            room.remove_entity(entity_key);
        }
    }

    /// Used to evaluate whether, given a User & Object that are in the
    /// same Room, said Object should be in scope for the given User.
    ///
    /// While Rooms allow for a very simple scope to which an Object can belong,
    /// this provides complete customization for advanced scopes.
    pub fn object_set_scope(
        &mut self,
        room_key: &RoomKey,
        user_key: &UserKey,
        object_key: &ObjectKey,
        in_scope: bool,
    ) {
        if !self.global_object_set.contains(object_key) {
            return;
        }
        let key = (*room_key, *user_key, *object_key);
        self.object_scope_map.insert(key, in_scope);
    }

    /// Similar to `object_set_scope()` but for entities only
    pub fn entity_set_scope(
        &mut self,
        room_key: &RoomKey,
        user_key: &UserKey,
        entity_key: &EntityKey,
        in_scope: bool,
    ) {
        let key = (*room_key, *user_key, *entity_key);
        self.entity_scope_map.insert(key, in_scope);
    }

    /// Return a collection of Object Scope Sets, being a unique combination of
    /// a related Room, User, and Object, used to determine which objects to
    /// replicate to which users
    pub fn object_scope_sets(&self) -> Vec<(RoomKey, UserKey, ObjectKey)> {
        let mut list: Vec<(RoomKey, UserKey, ObjectKey)> = Vec::new();

        // TODO: precache this, instead of generating a new list every call
        // likely this is called A LOT
        for (room_key, room) in self.rooms.iter() {
            for user_key in room.users_iter() {
                for object_key in room.objects_iter() {
                    list.push((room_key, *user_key, *object_key));
                }
            }
        }

        return list;
    }

    /// Return a collection of Entity Scope Sets, being a unique combination of
    /// a related Room, User, and Entity, used to determine which entities to
    /// replicate to which users
    pub fn entity_scope_sets(&self) -> Vec<(RoomKey, UserKey, EntityKey)> {
        let mut list: Vec<(RoomKey, UserKey, EntityKey)> = Vec::new();

        // TODO: precache this, instead of generating a new list every call
        // likely this is called A LOT
        for (room_key, room) in self.rooms.iter() {
            for user_key in room.users_iter() {
                for entity_key in room.entities_iter() {
                    list.push((room_key, *user_key, *entity_key));
                }
            }
        }

        return list;
    }

    /// Registers a closure which will be called during the handshake process
    /// with a new Client
    ///
    /// The Message evaluated in this closure should match the Message used
    /// client-side in the NaiaClient::new() method
    pub fn on_auth(&mut self, auth_func: Rc<Box<dyn Fn(&UserKey, &U) -> bool>>) {
        self.auth_func = Some(auth_func);
    }

    /// Iterate through all currently connected Users
    pub fn users_iter(&self) -> slotmap::dense::Iter<UserKey, User> {
        return self.users.iter();
    }

    /// Get a User, given the associated UserKey
    pub fn get_user(&self, user_key: &UserKey) -> Option<&User> {
        return self.users.get(*user_key);
    }

    /// Get the number of Users currently connected
    pub fn get_users_count(&self) -> usize {
        return self.users.len();
    }

    /// Gets the last received tick from the Client
    pub fn get_client_tick(&self, user_key: &UserKey) -> Option<u16> {
        if let Some(user_connection) = self.client_connections.get(user_key) {
            return Some(user_connection.get_last_received_tick());
        }
        return None;
    }

    /// Gets the current tick of the Server
    pub fn get_server_tick(&self) -> u16 {
        self.tick_manager.get_tick()
    }

    /// Returns true if a given User has an Object with a given ObjectKey
    /// in-scope currently
    pub fn user_scope_has_object(&self, user_key: &UserKey, object_key: &ObjectKey) -> bool {
        if let Some(user_connection) = self.client_connections.get(user_key) {
            return user_connection.has_object(object_key);
        }
        return false;
    }

    // Private methods

    fn update_object_scopes(&mut self) {
        for (room_key, room) in self.rooms.iter_mut() {
            while let Some((removed_user, removed_object)) = room.pop_object_removal_queue() {
                if let Some(user_connection) = self.client_connections.get_mut(&removed_user) {
                    Self::user_remove_object(user_connection, &removed_object);
                }
            }

            for user_key in room.users_iter() {
                for object_key in room.objects_iter() {
                    if let Some(user_connection) = self.client_connections.get_mut(user_key) {
                        let currently_in_scope = user_connection.has_object(object_key);

                        let should_be_in_scope: bool;
                        if user_connection.has_pawn(object_key) {
                            should_be_in_scope = true;
                        } else {
                            let key = (room_key, *user_key, *object_key);
                            if let Some(in_scope) = self.object_scope_map.get(&key) {
                                should_be_in_scope = *in_scope;
                            } else {
                                should_be_in_scope = false;
                            }
                        }

                        if should_be_in_scope {
                            if !currently_in_scope {
                                // add object to the connections local scope
                                if let Some(object) = self.global_replica_store.get(*object_key) {
                                    Self::user_add_object(user_connection, object_key, &object);
                                }
                            }
                        } else {
                            if currently_in_scope {
                                // remove object from the connections local scope
                                Self::user_remove_object(user_connection, object_key);
                            }
                        }
                    }
                }
            }
        }
    }

    fn update_entity_scopes(&mut self) {
        for (room_key, room) in self.rooms.iter_mut() {
            while let Some((removed_user, removed_entity)) = room.pop_entity_removal_queue() {
                if let Some(user_connection) = self.client_connections.get_mut(&removed_user) {
                    Self::user_remove_entity(user_connection, &removed_entity);
                }
            }

            for user_key in room.users_iter() {
                for entity_key in room.entities_iter() {
                    if self.entity_component_map.contains_key(entity_key) {
                        if let Some(user_connection) = self.client_connections.get_mut(user_key) {
                            let currently_in_scope = user_connection.has_entity(entity_key);

                            let should_be_in_scope: bool;
                            if user_connection.has_pawn_entity(entity_key) {
                                should_be_in_scope = true;
                            } else {
                                let key = (room_key, *user_key, *entity_key);
                                if let Some(in_scope) = self.entity_scope_map.get(&key) {
                                    should_be_in_scope = *in_scope;
                                } else {
                                    should_be_in_scope = false;
                                }
                            }

                            if should_be_in_scope {
                                if !currently_in_scope {
                                    // get a reference to the component map
                                    let component_set_ref =
                                        self.entity_component_map.get(entity_key).unwrap();

                                    // add entity to the connections local scope
                                    Self::user_add_entity(
                                        &self.global_replica_store,
                                        user_connection,
                                        entity_key,
                                        &component_set_ref,
                                    );
                                }
                            } else {
                                if currently_in_scope {
                                    // remove entity from the connections local scope
                                    Self::user_remove_entity(user_connection, entity_key);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn user_add_object(
        user_connection: &mut ClientConnection<U>,
        object_key: &ObjectKey,
        object_ref: &U,
    ) {
        //add object to user connection
        user_connection.add_object(object_key, &object_ref.inner_ref());
    }

    fn user_remove_object(user_connection: &mut ClientConnection<U>, object_key: &ObjectKey) {
        //remove object from user connection
        user_connection.remove_object(object_key);
    }

    fn user_add_entity(
        object_store: &DenseSlotMap<ObjectKey, U>,
        user_connection: &mut ClientConnection<U>,
        entity_key: &EntityKey,
        component_set_ref: &Ref<HashSet<ComponentKey>>,
    ) {
        // Get list of components first
        let mut component_list: Vec<(ComponentKey, Ref<dyn Replicate<U>>)> = Vec::new();
        let component_set: &HashSet<ComponentKey> = &component_set_ref.borrow();
        for component_key in component_set {
            if let Some(component_ref) = object_store.get(*component_key) {
                component_list.push((*component_key, component_ref.inner_ref().clone()));
            }
        }

        //add entity to user connection
        user_connection.add_entity(entity_key, component_set_ref, &component_list);
    }

    fn user_remove_entity(user_connection: &mut ClientConnection<U>, entity_key: &EntityKey) {
        //remove entity from user connection
        user_connection.remove_entity(entity_key);
    }

    fn user_add_component(
        user_connection: &mut ClientConnection<U>,
        entity_key: &EntityKey,
        component_key: &ComponentKey,
        component_ref: &Ref<dyn Replicate<U>>,
    ) {
        //add component to user connection
        user_connection.add_component(entity_key, component_key, component_ref);
    }

    async fn send_connect_accept_message(
        connection: &mut ClientConnection<U>,
        sender: &mut MessageSender,
    ) {
        let payload =
            connection.process_outgoing_header(0, 0, PacketType::ServerConnectResponse, &[]);
        match sender
            .send(Packet::new_raw(connection.get_address(), payload))
            .await
        {
            Ok(_) => {}
            Err(err) => {
                info!("send error! {}", err);
            }
        }
        connection.mark_sent();
    }

    async fn internal_send_connectionless(
        sender: &mut MessageSender,
        packet_type: PacketType,
        packet: Packet,
    ) {
        let new_payload =
            naia_shared::utils::write_connectionless_payload(packet_type, packet.payload());
        sender
            .send(Packet::new_raw(packet.address(), new_payload))
            .await
            .expect("send failed!");
    }
}

cfg_if! {
    if #[cfg(feature = "multithread")] {
        use std::sync::{Arc, Mutex};
        fn to_property_mutator_raw(eref: &Arc<Mutex<PropertyMutator>>) -> Arc<Mutex<dyn PropertyMutate>> {
            eref.clone()
        }
    } else {
        use std::cell::RefCell;
        fn to_property_mutator_raw(eref: &Rc<RefCell<PropertyMutator>>) -> Rc<RefCell<dyn PropertyMutate>> {
            eref.clone()
        }
    }
}

fn to_property_mutator(eref: &Ref<PropertyMutator>) -> Ref<dyn PropertyMutate> {
    let upcast_ref = to_property_mutator_raw(&eref.inner());
    Ref::new_raw(upcast_ref)
}
