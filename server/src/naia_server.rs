use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    rc::Rc,
};

use byteorder::{BigEndian, ReadBytesExt};
use log::info;
use ring::{hmac, rand};
use slotmap::DenseSlotMap;

use naia_server_socket::{
    Config as SocketConfig, MessageSender, Packet, ServerSocket, SocketEvent,
};
pub use naia_shared::{
    Config, Connection, Entity, EntityMutator, EntityType, Event, EventType, HostType, Instant,
    ManagerType, Manifest, PacketReader, PacketType, Timer, Timestamp,
};

use super::{
    client_connection::ClientConnection,
    entities::{
        entity_key::entity_key::EntityKey, mut_handler::MutHandler,
        server_entity_mutator::ServerEntityMutator,
    },
    error::NaiaServerError,
    room::{room_key::RoomKey, Room},
    server_event::ServerEvent,
    user::{user_key::UserKey, User},
};

/// A server that uses either UDP or WebRTC communication to send/receive events to/from connected clients, and syncs registered entities to clients to whom those entities are in-scope
pub struct NaiaServer<T: EventType, U: EntityType> {
    config: Config,
    manifest: Manifest<T, U>,
    socket: ServerSocket,
    sender: MessageSender,
    global_entity_store: DenseSlotMap<EntityKey, Rc<RefCell<dyn Entity<U>>>>,
    scope_entity_func: Option<Rc<Box<dyn Fn(&RoomKey, &UserKey, &EntityKey, U) -> bool>>>,
    auth_func: Option<Rc<Box<dyn Fn(&UserKey, &T) -> bool>>>,
    mut_handler: Rc<RefCell<MutHandler>>,
    users: DenseSlotMap<UserKey, User>,
    rooms: DenseSlotMap<RoomKey, Room>,
    address_to_user_key_map: HashMap<SocketAddr, UserKey>,
    client_connections: HashMap<UserKey, ClientConnection<T, U>>,
    outstanding_disconnects: VecDeque<UserKey>,
    heartbeat_timer: Timer,
    connection_hash_key: hmac::Key,
}

impl<T: EventType, U: EntityType> NaiaServer<T, U> {
    /// Create a new Server, given an address to listen at, an Event/Entity manifest, and an optional Config
    pub async fn new(address: &str, manifest: Manifest<T, U>, config: Option<Config>) -> Self {
        let mut config = match config {
            Some(config) => config,
            None => Config::default(),
        };
        config.heartbeat_interval /= 2;

        let mut socket_config = SocketConfig::default();
        socket_config.tick_interval = config.tick_interval;
        let mut server_socket = ServerSocket::listen(address, Some(socket_config)).await;

        let sender = server_socket.get_sender();
        let clients_map = HashMap::new();
        let heartbeat_timer = Timer::new(config.heartbeat_interval);

        let connection_hash_key =
            hmac::Key::generate(hmac::HMAC_SHA256, &rand::SystemRandom::new()).unwrap();

        NaiaServer {
            manifest,
            global_entity_store: DenseSlotMap::with_key(),
            scope_entity_func: None,
            auth_func: None,
            mut_handler: MutHandler::new(),
            socket: server_socket,
            sender,
            config,
            users: DenseSlotMap::with_key(),
            rooms: DenseSlotMap::with_key(),
            connection_hash_key,
            client_connections: clients_map,
            address_to_user_key_map: HashMap::new(),
            outstanding_disconnects: VecDeque::new(),
            heartbeat_timer,
        }
    }

    /// Must be called regularly, maintains connection to and receives messages from all Clients
    pub async fn receive(&mut self) -> Result<ServerEvent<T>, NaiaServerError> {
        let mut output: Option<Result<ServerEvent<T>, NaiaServerError>> = None;
        while output.is_none() {
            // heartbeats
            if self.heartbeat_timer.ringing() {
                self.heartbeat_timer.reset();

                for (user_key, connection) in self.client_connections.iter_mut() {
                    if let Some(user) = self.users.get(*user_key) {
                        if connection.should_drop() {
                            self.outstanding_disconnects.push_back(*user_key);
                        } else if connection.should_send_heartbeat() {
                            // Don't try to refactor this to self.internal_send, doesn't seem to work cause of iter_mut()
                            let payload =
                                connection.process_outgoing_header(PacketType::Heartbeat, &[]);
                            self.sender
                                .send(Packet::new_raw(user.address, payload))
                                .await
                                .expect("send failed!");
                            connection.mark_sent();
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
                output = Some(Ok(ServerEvent::Disconnection(user_key, user_clone)));
                continue;
            }

            for (address, connection) in self.client_connections.iter_mut() {
                //receive events from anyone
                if let Some(something) = connection.get_incoming_event() {
                    output = Some(Ok(ServerEvent::Event(*address, something)));
                    continue;
                }
            }

            //receive socket events
            match self.socket.receive().await {
                Ok(event) => {
                    match event {
                        SocketEvent::Packet(packet) => {
                            let address = packet.address();
                            if let Some(user_key) = self.address_to_user_key_map.get(&address) {
                                match self.client_connections.get_mut(&user_key) {
                                    Some(connection) => {
                                        connection.mark_heard();
                                    }
                                    None => {} //not yet established connection
                                }
                            }

                            let packet_type = PacketType::get_from_packet(packet.payload());

                            match packet_type {
                                PacketType::ClientChallengeRequest => {
                                    let payload = naia_shared::utils::read_headerless_payload(
                                        packet.payload(),
                                    );
                                    let mut reader = PacketReader::new(&payload);
                                    let timestamp = Timestamp::read(&mut reader);

                                    let mut timestamp_bytes = Vec::new();
                                    timestamp.write(&mut timestamp_bytes);
                                    let timestamp_hash: hmac::Tag =
                                        hmac::sign(&self.connection_hash_key, &timestamp_bytes);

                                    let mut payload_bytes = Vec::new();
                                    payload_bytes.append(&mut timestamp_bytes);
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
                                    let payload = naia_shared::utils::read_headerless_payload(
                                        packet.payload(),
                                    );
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
                                        //Verify that timestamp hash has been written by this server instance
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
                                            let buffer = reader.get_buffer();
                                            let cursor = reader.get_cursor();
                                            let naia_id_result = cursor.read_u16::<BigEndian>();
                                            if naia_id_result.is_err() {
                                                self.users.remove(user_key);
                                                continue;
                                            }
                                            let naia_id: u16 = naia_id_result.unwrap().into();
                                            let event_payload = buffer
                                                [cursor.position() as usize..buffer.len()]
                                                .to_vec()
                                                .into_boxed_slice();

                                            match self
                                                .manifest
                                                .create_event(naia_id, &event_payload)
                                            {
                                                Some(new_entity) => {
                                                    if !(auth_func.as_ref().as_ref())(
                                                        &user_key,
                                                        &new_entity,
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
                                            &self.config,
                                        );
                                        NaiaServer::<T, U>::send_connect_accept_message(
                                            &mut new_connection,
                                            &mut self.sender,
                                        )
                                        .await;
                                        self.client_connections.insert(user_key, new_connection);
                                        output = Some(Ok(ServerEvent::Connection(user_key)));
                                        continue;
                                    }
                                }
                                PacketType::Data => {
                                    if let Some(user_key) =
                                        self.address_to_user_key_map.get(&address)
                                    {
                                        match self.client_connections.get_mut(user_key) {
                                            Some(connection) => {
                                                let mut payload = connection
                                                    .process_incoming_header(packet.payload());
                                                connection.process_incoming_data(
                                                    &self.manifest,
                                                    &mut payload,
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
                                                // Still need to do this so that proper notify events fire based on the heartbeat header
                                                connection
                                                    .process_incoming_header(packet.payload());
                                                continue;
                                            }
                                            None => {
                                                warn!("received heartbeat from unauthenticated client: {}", address);
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        SocketEvent::Tick => {
                            // update entity scopes
                            self.update_entity_scopes();

                            // loop through all connections, send packet
                            for (user_key, connection) in self.client_connections.iter_mut() {
                                if let Some(user) = self.users.get(*user_key) {
                                    connection.collect_entity_updates();
                                    let mut packet_index: u8 = 1;
                                    while let Some(payload) =
                                        connection.get_outgoing_packet(&self.manifest)
                                    {
                                        info!("sending packet {}", packet_index);
                                        packet_index += 1;
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

                            output = Some(Ok(ServerEvent::Tick));
                            continue;
                        }
                    }
                }
                Err(error) => {
                    //                    //TODO: Determine if disconnecting a user based on a send error is the right thing to do
                    //                    if let NaiaServerSocketError::SendError(address) = error {
                    //                        if let Some(user_key) = self.address_to_user_key_map.get(&address).copied() {
                    //                            self.client_connections.remove(&user_key);
                    //                            output = Some(Ok(ServerEvent::Disconnection(user_key)));
                    //                            continue;
                    //                        }
                    //                    }

                    output = Some(Err(NaiaServerError::Wrapped(Box::new(error))));
                    continue;
                }
            }
        }
        return output.unwrap();
    }

    async fn send_connect_accept_message(
        connection: &mut ClientConnection<T, U>,
        sender: &mut MessageSender,
    ) {
        let payload = connection.process_outgoing_header(PacketType::ServerConnectResponse, &[]);
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

    /// Queues up an Event to be sent to the Client associated with a given UserKey
    pub fn send_event(&mut self, user_key: &UserKey, event: &impl Event<T>) {
        if let Some(connection) = self.client_connections.get_mut(user_key) {
            connection.queue_event(event);
        }
    }

    /// Register an Entity with the Server, whereby the Server will sync the state of the Entity to all connected
    /// Clients for which the Entity is in scope. Gives back an EntityKey which can be used to get the reference
    /// to the Entity from the Server once again
    pub fn register_entity(&mut self, entity: Rc<RefCell<dyn Entity<U>>>) -> EntityKey {
        let new_mutator_ref: Rc<RefCell<ServerEntityMutator>> =
            Rc::new(RefCell::new(ServerEntityMutator::new(&self.mut_handler)));
        entity
            .as_ref()
            .borrow_mut()
            .set_mutator(&to_entity_mutator(&new_mutator_ref));
        let entity_key = self.global_entity_store.insert(entity.clone());
        new_mutator_ref
            .as_ref()
            .borrow_mut()
            .set_entity_key(entity_key);
        self.mut_handler.borrow_mut().register_entity(&entity_key);
        return entity_key;
    }

    /// Deregisters an Entity with the Server, deleting local copies of the Entity on each Client
    pub fn deregister_entity(&mut self, key: EntityKey) {
        self.mut_handler.borrow_mut().deregister_entity(&key);
        self.global_entity_store.remove(key);
    }

    /// Given an EntityKey, get a reference to a registered Entity being tracked by the Server
    pub fn get_entity(&mut self, key: EntityKey) -> Option<&Rc<RefCell<dyn Entity<U>>>> {
        return self.global_entity_store.get(key);
    }

    /// Creates a new Room on the Server, returns a Key which can be used to reference said Room
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

    /// Add an Entity to a Room, given the appropriate RoomKey & EntityKey
    /// Entities will only ever be in-scope for Users which are in a Room with them
    pub fn room_add_entity(&mut self, room_key: &RoomKey, entity_key: &EntityKey) {
        if let Some(room) = self.rooms.get_mut(*room_key) {
            room.add_entity(entity_key);
        }
    }

    /// Add an User to a Room, given the appropriate RoomKey & UserKey
    /// Entities will only ever be in-scope for Users which are in a Room with them
    pub fn room_add_user(&mut self, room_key: &RoomKey, user_key: &UserKey) {
        if let Some(room) = self.rooms.get_mut(*room_key) {
            room.subscribe_user(user_key);
        }
    }

    /// Registers a closure which is used to evaluate whether, given a User & Entity that are in the same Room,
    /// said Entity should be in scope for the given User.
    ///
    /// While Rooms allow for a very simple scope to which an Entity can belong, this closure provides complete customization for advanced scopes.
    ///
    /// This closure will be called every Tick of the Server, for every User & Entity in a Room together, so try to keep it performant
    pub fn on_scope_entity(
        &mut self,
        scope_func: Rc<Box<dyn Fn(&RoomKey, &UserKey, &EntityKey, U) -> bool>>,
    ) {
        self.scope_entity_func = Some(scope_func);
    }

    /// Registers a closure which will be called during the handshake process with a new Client
    ///
    /// The Event evaluated in this closure should match the Event used client-side in the NaiaClient::new() method
    pub fn on_auth(&mut self, auth_func: Rc<Box<dyn Fn(&UserKey, &T) -> bool>>) {
        self.auth_func = Some(auth_func);
    }

    /// Get the current measured Round Trip Time to the Server
    pub fn get_rtt(&mut self, user_key: &UserKey) -> Option<f32> {
        if let Some(connection) = self.client_connections.get_mut(user_key) {
            return Some(connection.get_rtt());
        }
        return None;
    }

    /// Iterate through all currently connected Users
    pub fn users_iter(&self) -> slotmap::dense::Iter<UserKey, User> {
        return self.users.iter();
    }

    /// Get a User, given the associated UserKey
    pub fn get_user(&self, user_key: &UserKey) -> Option<&User> {
        return self.users.get(*user_key);
    }

    fn update_entity_scopes(&mut self) {
        for (room_key, room) in self.rooms.iter_mut() {
            while let Some((removed_user, removed_entity)) = room.pop_removal_queue() {
                if let Some(user_connection) = self.client_connections.get_mut(&removed_user) {
                    user_connection.remove_entity(&removed_entity);
                }
            }

            if let Some(scope_func) = &self.scope_entity_func {
                for user_key in room.users_iter() {
                    for entity_key in room.entities_iter() {
                        if let Some(entity) = self.global_entity_store.get(*entity_key) {
                            if let Some(user_connection) = self.client_connections.get_mut(user_key)
                            {
                                let currently_in_scope = user_connection.has_entity(entity_key);
                                let should_be_in_scope = (scope_func.as_ref().as_ref())(
                                    &room_key,
                                    user_key,
                                    entity_key,
                                    entity.as_ref().borrow().get_typed_copy(),
                                );
                                if should_be_in_scope {
                                    if !currently_in_scope {
                                        // add entity to the connections local scope
                                        if let Some(entity) =
                                            self.global_entity_store.get(*entity_key)
                                        {
                                            user_connection.add_entity(entity_key, entity);
                                        }
                                    }
                                } else {
                                    if currently_in_scope {
                                        // remove entity from the connections local scope
                                        user_connection.remove_entity(entity_key);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
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

fn to_entity_mutator(eref: &Rc<RefCell<ServerEntityMutator>>) -> Rc<RefCell<dyn EntityMutator>> {
    eref.clone()
}
