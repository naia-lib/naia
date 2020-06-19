
use std::{
    net::SocketAddr,
    collections::{VecDeque, HashMap},
    rc::Rc,
    cell::RefCell,
};

use log::{info};
use slotmap::{DenseSlotMap};
use ring::{hmac, rand};

use gaia_server_socket::{ServerSocket, SocketEvent, MessageSender, Config as SocketConfig, GaiaServerSocketError};
pub use gaia_shared::{Config, PacketType, Connection, Timer, Timestamp, Manifest, PacketReader,
                      Event, Entity, ManagerType, HostType, EventType, EntityType, EntityMutator};

use super::{
    Packet,
    error::GaiaServerError,
    server_event::ServerEvent,
    client_connection::ClientConnection,
    entities::{
        mut_handler::MutHandler,
        entity_key::EntityKey,
        server_entity_mutator::ServerEntityMutator,
    },
    room::{Room, RoomKey},
    user::{User, UserKey},
};

pub struct GaiaServer<T: EventType, U: EntityType> {
    config: Config,
    manifest: Manifest<T, U>,
    socket: ServerSocket,
    sender: MessageSender,
    global_entity_store: DenseSlotMap<EntityKey, Rc<RefCell<dyn Entity<U>>>>,
    scope_entity_func: Option<Rc<Box<dyn Fn(&RoomKey, &UserKey, &EntityKey, U) -> bool>>>,
    mut_handler: Rc<RefCell<MutHandler>>,
    users: DenseSlotMap<UserKey, User>,
    rooms: DenseSlotMap<RoomKey, Room>,
    address_to_user_key_map: HashMap<SocketAddr, UserKey>,
    client_connections: HashMap<UserKey, ClientConnection<T, U>>,
    outstanding_disconnects: VecDeque<UserKey>,
    heartbeat_timer: Timer,
    connection_hash_key: hmac::Key,
    drop_counter: u8,
    drop_max: u8,
}

impl<T: EventType, U: EntityType> GaiaServer<T, U> {
    pub async fn listen(address: &str, manifest: Manifest<T, U>, config: Option<Config>) -> Self {

        let mut config = match config {
            Some(config) => config,
            None => Config::default()
        };
        config.heartbeat_interval /= 2;

        let mut socket_config = SocketConfig::default();
        socket_config.connectionless = true;
        socket_config.tick_interval = config.tick_interval;
        let mut server_socket = ServerSocket::listen(address, Some(socket_config)).await;

        let sender = server_socket.get_sender();
        let clients_map = HashMap::new();
        let heartbeat_timer = Timer::new(config.heartbeat_interval);

        let connection_hash_key = hmac::Key::generate(hmac::HMAC_SHA256, &rand::SystemRandom::new()).unwrap();

        GaiaServer {
            manifest,
            global_entity_store: DenseSlotMap::with_key(),
            scope_entity_func: None,
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
            drop_counter: 1,
            drop_max: 2,
        }
    }

    pub async fn receive(&mut self) -> Result<ServerEvent<T>, GaiaServerError> {
        let mut output: Option<Result<ServerEvent<T>, GaiaServerError>> = None;
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
                            let payload = connection.process_outgoing_header(PacketType::Heartbeat, &[]);
                            self.sender.send(Packet::new_raw(user.address, payload))
                                .await
                                .expect("send failed!");
                            connection.mark_sent();
                        }
                    }
                }
            }

            // timeouts
            if let Some(user_key) = self.outstanding_disconnects.pop_front() {
                self.client_connections.remove(&user_key);
                output = Some(Ok(ServerEvent::Disconnection(user_key)));
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
                            if packet_type == PacketType::Data {
                                //simulate dropping
                                if self.drop_counter >= self.drop_max {
                                    self.drop_counter = 0;
                                    info!("~~~~~~~~~~  dropped packet from client  ~~~~~~~~~~");
                                    continue;
                                } else {
                                    self.drop_counter += 1;
                                }
                            }

                            match packet_type {
                                PacketType::ClientChallengeRequest => {
                                    let payload = gaia_shared::utils::read_headerless_payload(packet.payload());
                                    let mut reader = PacketReader::new(&payload);
                                    let timestamp = Timestamp::read(&mut reader);

                                    if !self.address_to_user_key_map.contains_key(&address) {
                                        let user = User::new(address, timestamp);
                                        let user_key = self.users.insert(user);
                                        self.address_to_user_key_map.insert(address, user_key);
                                    }

                                    let mut timestamp_bytes = Vec::new();
                                    timestamp.write(&mut timestamp_bytes);
                                    let timestamp_hash: hmac::Tag = hmac::sign(&self.connection_hash_key, &timestamp_bytes);

                                    let mut payload_bytes = Vec::new();
                                    payload_bytes.append(&mut timestamp_bytes);
                                    let hash_bytes: &[u8] = timestamp_hash.as_ref();
                                    for hash_byte in hash_bytes {
                                        payload_bytes.push(*hash_byte);
                                    }

                                    GaiaServer::<T,U>::internal_send_connectionless(
                                        &mut self.sender,
                                        PacketType::ServerChallengeResponse,
                                        Packet::new(address, payload_bytes))
                                        .await;
                                }
                                PacketType::ClientConnectRequest => {
                                    let payload = gaia_shared::utils::read_headerless_payload(packet.payload());
                                    let mut reader = PacketReader::new(&payload);
                                    let timestamp = Timestamp::read(&mut reader);

                                    if let Some(user_key) = self.address_to_user_key_map.get(&address) {
                                        if let Some(user) = self.users.get(*user_key) {
                                           if user.timestamp == timestamp {
                                               if self.client_connections.contains_key(user_key) {
                                                   let connection = self.client_connections.get_mut(user_key).unwrap();
                                                   let payload = connection.process_outgoing_header(
                                                       PacketType::ServerConnectResponse,
                                                       &[]);
                                                   match self.sender.send(Packet::new_raw(address, payload))
                                                       .await {
                                                       Ok(_) => {}
                                                       Err(err) => {
                                                           info!("send error! {}", err);
                                                       }
                                                   }
                                                   connection.mark_sent();
                                                   continue;
                                               } else {
                                                   let mut timestamp_bytes: Vec<u8> = Vec::new();
                                                   timestamp.write(&mut timestamp_bytes);
                                                   let mut digest_bytes: Vec<u8> = Vec::new();
                                                   for _ in 0..32 {
                                                       digest_bytes.push(reader.read_u8());
                                                   }
                                                   if hmac::verify(&self.connection_hash_key, &timestamp_bytes, &digest_bytes).is_ok() {
                                                       // Success!
                                                       self.client_connections.insert(*user_key,
                                                                                      ClientConnection::new(
                                                                                          address,
                                                                                          Some(&self.mut_handler),
                                                                                          self.config.heartbeat_interval,
                                                                                          self.config.disconnection_timeout_duration));
                                                       output = Some(Ok(ServerEvent::Connection(*user_key)));
                                                       //TODO: send a connect request here instead of relying on the client to send a second request..
                                                       continue;
                                                   }
                                               }
                                           }
                                        }
                                    }
                                }
                                PacketType::Data => {
                                    if let Some(user_key) = self.address_to_user_key_map.get(&address) {
                                        match self.client_connections.get_mut(user_key) {
                                            Some(connection) => {
                                                let mut payload = connection.process_incoming_header(packet.payload());
                                                connection.process_incoming_data(&self.manifest, &mut payload);
                                                continue;
                                            }
                                            None => {
                                                warn!("received data from unauthenticated client: {}", address);
                                            }
                                        }
                                    }
                                }
                                PacketType::Heartbeat => {
                                    if let Some(user_key) = self.address_to_user_key_map.get(&address) {
                                        match self.client_connections.get_mut(user_key) {
                                            Some(connection) => {
                                                // Still need to do this so that proper notify events fire based on the heartbeat header
                                                connection.process_incoming_header(packet.payload());
                                                info!("<- c");
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
                                    while let Some(payload) = connection.get_outgoing_packet(&self.manifest) {
                                        info!("sending packet {}", packet_index);
                                        packet_index += 1;
                                        match self.sender.send(Packet::new_raw(user.address, payload))
                                            .await {
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
                        _ => {} // We are not using Socket Connection/Disconnection Events
                    }
                }
                Err(error) => {
                    if let GaiaServerSocketError::SendError(address) = error {
                        if let Some(user_key) = self.address_to_user_key_map.get(&address).copied() {
                            self.client_connections.remove(&user_key);
                            output = Some(Ok(ServerEvent::Disconnection(user_key)));
                            continue;
                        }
                    }

                    output = Some(Err(GaiaServerError::Wrapped(Box::new(error))));
                    continue;
                }
            }
        }
        return output.unwrap();
    }

    pub fn send_event(&mut self, user_key: &UserKey, event: &impl Event<T>) {
        if let Some(connection) = self.client_connections.get_mut(user_key) {
            connection.queue_event(event);
        }
    }

    pub fn register_entity(&mut self, entity: &Rc<RefCell<dyn Entity<U>>>) -> EntityKey {
        let new_mutator_ref: Rc<RefCell<ServerEntityMutator>> = Rc::new(RefCell::new(ServerEntityMutator::new(&self.mut_handler)));
        entity.as_ref().borrow_mut().set_mutator(&to_entity_mutator(&new_mutator_ref));
        let entity_key = self.global_entity_store.insert(entity.clone());
        new_mutator_ref.as_ref().borrow_mut().set_entity_key(entity_key);
        self.mut_handler.borrow_mut().register_entity(&entity_key);
        return entity_key
    }

    pub fn deregister_entity(&mut self, key: EntityKey) {
        self.mut_handler.borrow_mut().deregister_entity(&key);
        self.global_entity_store.remove(key);
    }

    pub fn get_entity(&mut self, key: EntityKey) -> Option<&Rc<RefCell<dyn Entity<U>>>> {
        return self.global_entity_store.get(key);
    }

    pub fn create_room(&mut self) -> RoomKey {
        let new_room = Room::new();
        return self.rooms.insert(new_room);
    }

    pub fn delete_room(&mut self, key: RoomKey) {
        self.rooms.remove(key);
    }

    pub fn get_room(&self, key: RoomKey) -> Option<&Room> {
        return self.rooms.get(key);
    }

    pub fn get_room_mut(&mut self, key: RoomKey) -> Option<&mut Room> {
        return self.rooms.get_mut(key);
    }

    pub fn rooms_iter(&self) -> slotmap::dense::Iter<RoomKey, Room> {
        return self.rooms.iter();
    }

    pub fn on_scope_entity(&mut self, scope_func: Rc<Box<dyn Fn(&RoomKey, &UserKey, &EntityKey, U) -> bool>>) {
        self.scope_entity_func = Some(scope_func);
    }

    pub fn get_sequence_number(&mut self, user_key: &UserKey) -> Option<u16> {
        if let Some(connection) = self.client_connections.get_mut(user_key) {
            return Some(connection.get_next_packet_index());
        }
        return None;
    }

    pub fn users_iter(&self) -> slotmap::dense::Iter<UserKey, User> {
        return self.users.iter();
    }

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
                            if let Some(user_connection) = self.client_connections.get_mut(user_key) {
                                let currently_in_scope = user_connection.has_entity(entity_key);
                                let should_be_in_scope = (scope_func.as_ref().as_ref())(&room_key, user_key, entity_key, entity.as_ref().borrow().to_type());
                                if should_be_in_scope {
                                    if !currently_in_scope {
                                        // add entity to the connections local scope
                                        if let Some(entity) = self.global_entity_store.get(*entity_key) {
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

    async fn internal_send_connectionless(sender: &mut MessageSender, packet_type: PacketType, packet: Packet) {
        let new_payload = gaia_shared::utils::write_connectionless_payload(packet_type, packet.payload());
        sender.send(Packet::new_raw(packet.address(), new_payload))
            .await
            .expect("send failed!");
    }
}

fn to_entity_mutator(eref: &Rc<RefCell<ServerEntityMutator>>) -> Rc<RefCell<dyn EntityMutator>> {
    eref.clone()
}