use std::{
    collections::{HashSet, HashMap, VecDeque},
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
    wrapping_diff, Actor, ActorMutator, ActorType, Connection, ConnectionConfig, Event, EventType,
    HostTickManager, Instant, ManagerType, Manifest, PacketReader, PacketType, Ref, SharedConfig,
    Timer, Timestamp, LocalActorKey
};

use super::{
    actors::{
        actor_key::actor_key::ActorKey, mut_handler::MutHandler,
        server_actor_mutator::ServerActorMutator,
    },
    client_connection::ClientConnection,
    error::NaiaServerError,
    interval::Interval,
    room::{room_key::RoomKey, Room},
    server_config::ServerConfig,
    server_event::ServerEvent,
    server_tick_manager::ServerTickManager,
    user::{user_key::UserKey, User},
};
use naia_shared::{StandardHeader, KeyGenerator, EntityKey};

/// A server that uses either UDP or WebRTC communication to send/receive events
/// to/from connected clients, and syncs registered actors to clients to whom
/// those actors are in-scope
pub struct NaiaServer<T: EventType, U: ActorType> {
    connection_config: ConnectionConfig,
    manifest: Manifest<T, U>,
    socket: Box<dyn ServerSocketTrait>,
    sender: MessageSender,
    global_actor_store: DenseSlotMap<ActorKey, U>,
    scope_actor_func: Option<Rc<Box<dyn Fn(&RoomKey, &UserKey, &ActorKey, U) -> bool>>>,
    auth_func: Option<Rc<Box<dyn Fn(&UserKey, &T) -> bool>>>,
    mut_handler: Ref<MutHandler>,
    users: DenseSlotMap<UserKey, User>,
    rooms: DenseSlotMap<RoomKey, Room>,
    address_to_user_key_map: HashMap<SocketAddr, UserKey>,
    client_connections: HashMap<UserKey, ClientConnection<T, U>>,
    outstanding_disconnects: VecDeque<UserKey>,
    heartbeat_timer: Timer,
    connection_hash_key: hmac::Key,
    tick_manager: ServerTickManager,
    tick_timer: Interval,
    scope_change_events: VecDeque<ScopeEvent>,
    entity_key_generator: KeyGenerator,
    entity_key_store: HashSet<EntityKey>,
}

enum ScopeEvent {
    ActorIntoScope(UserKey,   ActorKey),
    ActorOutOfScope(UserKey,  ActorKey),
    EntityIntoScope(UserKey,  EntityKey),
    EntityOutOfScope(UserKey, EntityKey),
}

/// A collection of IP addresses describing which IP to listen on for new
/// sessions, which to dedicate to UDP traffic, and which to advertise publicly
pub struct ServerAddresses {
    session_listen_addr: SocketAddr,
    webrtc_listen_addr: SocketAddr,
    public_webrtc_addr: SocketAddr,
}

impl ServerAddresses {
    /// Create a new ServerAddresses config struct from component addresses
    pub fn new(
        session_listen_addr: SocketAddr,
        webrtc_listen_addr: SocketAddr,
        public_webrtc_addr: SocketAddr,
    ) -> Self {
        ServerAddresses {
            session_listen_addr,
            webrtc_listen_addr,
            public_webrtc_addr,
        }
    }
}

impl<T: EventType, U: ActorType> NaiaServer<T, U> {
    /// Create a new Server, given an address to listen at, an Event/Actor
    /// manifest, and an optional Config
    pub async fn new(
        addresses: ServerAddresses,
        manifest: Manifest<T, U>,
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
            addresses.session_listen_addr,
            addresses.webrtc_listen_addr,
            addresses.public_webrtc_addr,
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

        NaiaServer {
            manifest,
            global_actor_store: DenseSlotMap::with_key(),
            scope_actor_func: None,
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
            tick_manager: ServerTickManager::new(shared_config.tick_interval),
            tick_timer: Interval::new(shared_config.tick_interval),
            scope_change_events: VecDeque::new(),
            entity_key_generator: KeyGenerator::new(),
            entity_key_store: HashSet::new(),
        }
    }

    /// Must be called regularly, maintains connection to and receives messages
    /// from all Clients
    pub async fn receive(&mut self) -> Result<ServerEvent<T>, NaiaServerError> {
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

                return Ok(ServerEvent::Disconnection(user_key, user_clone));
            }

            // TODO: have 1 single queue for commands/events from all users, as it's
            // possible this current technique unfairly favors the 1st users in
            // self.client_connections
            for (user_key, connection) in self.client_connections.iter_mut() {
                //receive commands from anyone
                if let Some((pawn_key, command)) =
                    connection.get_incoming_command(self.tick_manager.get_tick())
                {
                    return Ok(ServerEvent::Command(*user_key, pawn_key, command));
                }
                //receive events from anyone
                if let Some(event) = connection.get_incoming_event() {
                    return Ok(ServerEvent::Event(*user_key, event));
                }
            }

            //receive scope change events events
            if let Some(event) = self.scope_change_events.pop_front() {
                match event {
                    ScopeEvent::ActorIntoScope(user_key,  actor_key) =>   return Ok(ServerEvent::IntoScope(user_key, actor_key)),
                    ScopeEvent::ActorOutOfScope(user_key, actor_key) =>  return Ok(ServerEvent::OutOfScope(user_key, actor_key)),
                    ScopeEvent::EntityIntoScope(user_key,  entity_key) =>  return Ok(ServerEvent::IntoScopeEntity(user_key, entity_key)),
                    ScopeEvent::EntityOutOfScope(user_key, entity_key) => return Ok(ServerEvent::OutOfScopeEntity(user_key, entity_key)),
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

                                    NaiaServer::<T, U>::internal_send_connectionless(
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
                                                NaiaServer::<T, U>::send_connect_accept_message(
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

                                            match self.manifest.create_event(naia_id, &mut reader) {
                                                Some(new_actor) => {
                                                    if !(auth_func.as_ref().as_ref())(
                                                        &user_key, &new_actor,
                                                    ) {
                                                        self.users.remove(user_key);
                                                        continue;
                                                    }
                                                }
                                                _ => {
                                                    self.users.remove(user_key);
                                                    continue;
                                                }
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
                                        NaiaServer::<T, U>::send_connect_accept_message(
                                            &mut new_connection,
                                            &mut self.sender,
                                        )
                                        .await;
                                        self.client_connections.insert(user_key, new_connection);
                                        return Ok(ServerEvent::Connection(user_key));
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
                            // Some(Ok(ServerEvent::Disconnection(user_key)));
                            //                            continue;
                            //                        }
                            //                    }

                            return Err(NaiaServerError::Wrapped(Box::new(error)));
                        }
                    }
                }
                Next::Tick => {
                    self.tick_manager.increment_tick();
                    return Ok(ServerEvent::Tick);
                }
            }
        }
    }



    /// Queues up an Event to be sent to the Client associated with a given
    /// UserKey
    pub fn queue_event(&mut self, user_key: &UserKey, event: &impl Event<T>) {
        if let Some(connection) = self.client_connections.get_mut(user_key) {
            connection.queue_event(event);
        }
    }

    /// Sends all Actor/Event messages to all Clients. If you don't call this
    /// method, the Server will never communicate with it's connected
    /// Clients
    pub async fn send_all_updates(&mut self) {
        // update actor scopes
        self.update_actor_scopes();

        // loop through all connections, send packet
        for (user_key, connection) in self.client_connections.iter_mut() {
            if let Some(user) = self.users.get(*user_key) {
                connection.collect_actor_updates();
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

    /// Register an Actor with the Server, whereby the Server will sync the
    /// state of the Actor to all connected Clients for which the Actor is
    /// in scope. Gives back an ActorKey which can be used to get the reference
    /// to the Actor from the Server once again
    pub fn register_actor(&mut self, actor: U) -> ActorKey {
        let new_mutator_ref: Ref<ServerActorMutator> =
            Ref::new(ServerActorMutator::new(&self.mut_handler));
        actor
            .inner_ref()
            .borrow_mut()
            .set_mutator(&to_actor_mutator(&new_mutator_ref));
        let actor_key = self.global_actor_store.insert(actor);
        new_mutator_ref.borrow_mut().set_actor_key(actor_key);
        self.mut_handler.borrow_mut().register_actor(&actor_key);
        return actor_key;
    }

    /// Deregisters an Actor with the Server, deleting local copies of the
    /// Actor on each Client
    pub fn deregister_actor(&mut self, key: ActorKey) {
        for (user_key, _) in self.users.iter() {
            if let Some(user_connection) = self.client_connections.get_mut(&user_key) {
                user_connection.remove_pawn(&key);
                Self::user_remove_actor(&mut self.scope_change_events,
                                        user_connection,
                                        &user_key,
                                        &key);
            }
        }

        self.mut_handler.borrow_mut().deregister_actor(&key);
        self.global_actor_store.remove(key);
    }

    /// Given an ActorKey, get a reference to a registered Actor being tracked
    /// by the Server
    pub fn get_actor(&mut self, key: ActorKey) -> Option<&U> {
        return self.global_actor_store.get(key);
    }

    /// Iterate through all the Server's Actors
    pub fn actors_iter(&self) -> slotmap::dense::Iter<ActorKey, U> {
        return self.global_actor_store.iter();
    }

    /// Get the number of Actors tracked by the Server
    pub fn get_actors_count(&self) -> usize {
        return self.global_actor_store.len();
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

    /// Add an Actor to a Room, given the appropriate RoomKey & ActorKey
    /// Actors will only ever be in-scope for Users which are in a Room with
    /// them
    pub fn room_add_actor(&mut self, room_key: &RoomKey, actor_key: &ActorKey) {
        if let Some(room) = self.rooms.get_mut(*room_key) {
            room.add_actor(actor_key);
        }
    }

    /// Remove an Actor from a Room, given the appropriate RoomKey & ActorKey
    pub fn room_remove_actor(&mut self, room_key: &RoomKey, actor_key: &ActorKey) {
        if let Some(room) = self.rooms.get_mut(*room_key) {
            room.remove_actor(actor_key);
        }
    }

    /// Add an User to a Room, given the appropriate RoomKey & UserKey
    /// Actors will only ever be in-scope for Users which are in a Room with
    /// them
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

    /// Registers a closure which is used to evaluate whether, given a User &
    /// Actor that are in the same Room, said Actor should be in scope for
    /// the given User.
    ///
    /// While Rooms allow for a very simple scope to which an Actor can belong,
    /// this closure provides complete customization for advanced scopes.
    ///
    /// This closure will be called every Tick of the Server, for every User &
    /// Actor in a Room together, so try to keep it performant
    pub fn on_scope_actor(
        &mut self,
        scope_func: Rc<Box<dyn Fn(&RoomKey, &UserKey, &ActorKey, U) -> bool>>,
    ) {
        self.scope_actor_func = Some(scope_func);
    }

    /// Registers a closure which will be called during the handshake process
    /// with a new Client
    ///
    /// The Event evaluated in this closure should match the Event used
    /// client-side in the NaiaClient::new() method
    pub fn on_auth(&mut self, auth_func: Rc<Box<dyn Fn(&UserKey, &T) -> bool>>) {
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

    /// Assigns an Actor to a specific User, making it a Pawn for that User
    /// (meaning that the User will be able to issue Commands to that Pawn)
    pub fn assign_pawn(&mut self, user_key: &UserKey, actor_key: &ActorKey) {
        if self.global_actor_store.contains_key(*actor_key) {
            if let Some(user_connection) = self.client_connections.get_mut(user_key) {
                user_connection.add_pawn(actor_key);
            }
        }
    }

    /// Unassigns a Pawn from a specific User (meaning that the User will be
    /// unable to issue Commands to that Pawn)
    pub fn unassign_pawn(&mut self, user_key: &UserKey, actor_key: &ActorKey) {
        if self.global_actor_store.contains_key(*actor_key) {
            if let Some(user_connection) = self.client_connections.get_mut(user_key) {
                user_connection.remove_pawn(actor_key);
            }
        }
    }

    /// Returns true if a given User has an Actor with a given ActorKey in-scope currently
    pub fn user_scope_has_actor(&self, user_key: &UserKey, actor_key: &ActorKey) -> bool {
        if let Some(user_connection) = self.client_connections.get(user_key) {
            return user_connection.has_actor(actor_key);
        }
        return false;
    }

    /// Returns the local key used to reference a given actor for a given user
    pub fn get_user_local_key_for_actor(&self, user_key: &UserKey, actor_key: &ActorKey) -> Option<LocalActorKey> {
        if let Some(user_connection) = self.client_connections.get(user_key) {
            return user_connection.get_actor_local_key(actor_key);
        }
        return None;
    }

    /// see if actor is created for given user
    pub fn actor_is_created(&self, user_key: &UserKey, local_key: &LocalActorKey) -> bool {
        if let Some(user_connection) = self.client_connections.get(user_key) {
            return user_connection.actor_is_created(local_key);
        }

        return false;
    }

    /// Register an Entity with the Server, whereby the Server will sync the
    /// state of all the given Entity's Components to all connected Clients for which the Entity is
    /// in scope. Gives back an EntityKey which can be used to get the reference
    /// to the Entity from the Server once again
    pub fn register_entity(&mut self) -> EntityKey {
        let entity_key: EntityKey = self.entity_key_generator.generate();
        self.entity_key_store.insert(entity_key);
        return entity_key;
    }

    /// Deregisters an Entity with the Server, deleting local copies of the
    /// Entity on each Client
    pub fn deregister_entity(&mut self, key: &EntityKey) {
        for (user_key, _) in self.users.iter() {
            if let Some(user_connection) = self.client_connections.get_mut(&user_key) {
                user_connection.remove_pawn_entity(key);
                Self::user_remove_entity(&mut self.scope_change_events,
                                        user_connection,
                                        &user_key,
                                        key);
            }
        }

        self.entity_key_store.remove(key);
    }

    // Private methods

    fn update_actor_scopes(&mut self) {
        for (room_key, room) in self.rooms.iter_mut() {
            while let Some((removed_user, removed_actor)) = room.pop_removal_queue() {
                if let Some(user_connection) = self.client_connections.get_mut(&removed_user) {
                    Self::user_remove_actor(&mut self.scope_change_events,
                                            user_connection,
                                            &removed_user,
                                            &removed_actor);
                }
            }

            if let Some(scope_func) = &self.scope_actor_func {
                for user_key in room.users_iter() {
                    for actor_key in room.actors_iter() {
                        if let Some(actor) = self.global_actor_store.get(*actor_key) {
                            if let Some(user_connection) = self.client_connections.get_mut(user_key)
                            {
                                let currently_in_scope = user_connection.has_actor(actor_key);
                                let should_be_in_scope = user_connection.has_pawn(actor_key)
                                    || (scope_func.as_ref().as_ref())(
                                        &room_key,
                                        user_key,
                                        actor_key,
                                        (*actor).clone(),
                                    );
                                if should_be_in_scope {
                                    if !currently_in_scope {
                                        // add actor to the connections local scope
                                        if let Some(actor) = self.global_actor_store.get(*actor_key)
                                        {
                                            Self::user_add_actor(&mut self.scope_change_events,
                                                                 user_connection,
                                                                 user_key,
                                                                 actor_key,
                                                                 &actor);
                                        }
                                    }
                                } else {
                                    if currently_in_scope {
                                        // remove actor from the connections local scope
                                        Self::user_remove_actor(&mut self.scope_change_events,
                                                                user_connection,
                                                                user_key,
                                                                actor_key);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn user_add_actor(event_queue: &mut VecDeque<ScopeEvent>,
                      user_connection: &mut ClientConnection<T, U>,
                      user_key: &UserKey,
                      actor_key: &ActorKey,
                      actor_ref: &U) {
        //remove actor from user connection
        user_connection.add_actor(actor_key, &actor_ref.inner_ref());

        //fire event
        event_queue.push_back(ScopeEvent::ActorIntoScope(*user_key, *actor_key));
    }

    fn user_remove_actor(event_queue: &mut VecDeque<ScopeEvent>,
                         user_connection: &mut ClientConnection<T, U>,
                         user_key: &UserKey,
                         actor_key: &ActorKey) {
        //add actor to user connection
        user_connection.remove_actor(actor_key);

        //fire event
        event_queue.push_back(ScopeEvent::ActorOutOfScope(*user_key, *actor_key));
    }

    fn user_add_entity(event_queue: &mut VecDeque<ScopeEvent>,
                      user_connection: &mut ClientConnection<T, U>,
                      user_key: &UserKey,
                      entity_key: &EntityKey) {
        //add entity to user connection
        user_connection.add_entity(entity_key);

        //fire event
        event_queue.push_back(ScopeEvent::EntityIntoScope(*user_key, *entity_key));
    }

    fn user_remove_entity(event_queue: &mut VecDeque<ScopeEvent>,
                         user_connection: &mut ClientConnection<T, U>,
                         user_key: &UserKey,
                         entity_key: &EntityKey) {
        //remove actor from user connection
        user_connection.remove_entity(entity_key);

        //fire event
        event_queue.push_back(ScopeEvent::EntityOutOfScope(*user_key, *entity_key));
    }

    async fn send_connect_accept_message(
        connection: &mut ClientConnection<T, U>,
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
        fn to_actor_mutator_raw(eref: &Arc<Mutex<ServerActorMutator>>) -> Arc<Mutex<dyn ActorMutator>> {
            eref.clone()
        }
    } else {
        use std::cell::RefCell;
        fn to_actor_mutator_raw(eref: &Rc<RefCell<ServerActorMutator>>) -> Rc<RefCell<dyn ActorMutator>> {
            eref.clone()
        }
    }
}

fn to_actor_mutator(eref: &Ref<ServerActorMutator>) -> Ref<dyn ActorMutator> {
    let upcast_ref = to_actor_mutator_raw(&eref.inner());
    Ref::new_raw(upcast_ref)
}
