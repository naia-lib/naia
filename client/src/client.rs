use std::{collections::VecDeque, net::SocketAddr};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use naia_client_socket::{ClientSocket, PacketReceiver, PacketSender};

pub use naia_shared::{
    ConnectionConfig, HostTickManager, ImplRef, Instant, LocalComponentKey, LocalEntityKey,
    LocalObjectKey, LocalReplicaKey, ManagerType, Manifest, PacketReader, PacketType, PawnKey,
    ProtocolType, Ref, Replicate, SequenceIterator, SharedConfig, StandardHeader, Timer, Timestamp,
};

use super::{
    client_config::ClientConfig,
    connection_state::{ConnectionState, ConnectionState::AwaitingChallengeResponse},
    error::NaiaClientError,
    event::Event,
    replica_action::ReplicaAction,
    server_connection::ServerConnection,
    tick_manager::TickManager,
    Packet,
};

/// Client can send/receive messages to/from a server, and has a pool of
/// in-scope objects/entities/components that are synced with the server
#[derive(Debug)]
pub struct Client<T: ProtocolType> {
    manifest: Manifest<T>,
    server_address: SocketAddr,
    connection_config: ConnectionConfig,
    sender: PacketSender,
    receiver: Box<dyn PacketReceiver>,
    server_connection: Option<ServerConnection<T>>,
    pre_connection_timestamp: Option<Timestamp>,
    pre_connection_digest: Option<Box<[u8]>>,
    handshake_timer: Timer,
    connection_state: ConnectionState,
    auth_message: Option<Ref<dyn Replicate<T>>>,
    tick_manager: TickManager,
    outstanding_connect: bool,
    outstanding_errors: VecDeque<NaiaClientError>,
}

impl<T: ProtocolType> Client<T> {
    /// Create a new Client
    pub fn new<U: ImplRef<T>>(
        manifest: Manifest<T>,
        client_config: Option<ClientConfig>,
        shared_config: SharedConfig,
        auth: Option<U>,
    ) -> Self {
        let mut client_config = match client_config {
            Some(config) => config,
            None => ClientConfig::default(),
        };
        client_config.socket_config.shared.link_condition_config =
            shared_config.link_condition_config.clone();

        let server_address = client_config.socket_config.server_address;

        let connection_config = ConnectionConfig::new(
            client_config.disconnection_timeout_duration,
            client_config.heartbeat_interval,
            client_config.ping_interval,
            client_config.rtt_sample_size,
        );

        let (sender, receiver) = ClientSocket::connect(client_config.socket_config);

        let mut handshake_timer = Timer::new(client_config.send_handshake_interval);
        handshake_timer.ring_manual();

        let auth_message: Option<Ref<dyn Replicate<T>>> = {
            if auth.is_none() {
                None
            } else {
                Some(auth.unwrap().dyn_ref())
            }
        };

        Client {
            server_address,
            manifest,
            sender,
            receiver,
            connection_config,
            handshake_timer,
            server_connection: None,
            pre_connection_timestamp: None,
            pre_connection_digest: None,
            connection_state: AwaitingChallengeResponse,
            auth_message,
            tick_manager: TickManager::new(shared_config.tick_interval),
            outstanding_connect: false,
            outstanding_errors: VecDeque::new(),
        }
    }

    /// Must call this regularly (preferably at the beginning of every draw
    /// frame), in a loop until it returns None.
    /// Retrieves incoming update data, and maintains the connection.
    pub fn receive(&mut self) -> VecDeque<Result<Event<T>, NaiaClientError>> {
        let mut events = VecDeque::new();

        // Need to run this to maintain connection with server, and receive packets
        // until none left
        self.maintain_socket();

        // send ticks, handshakes, heartbeats, pings, timeout if need be
        match &mut self.server_connection {
            Some(connection) => {
                // return connect event
                if self.outstanding_connect {
                    events.push_back(Ok(Event::Connection));
                    self.outstanding_connect = false;
                }
                // new errors
                while let Some(err) = self.outstanding_errors.pop_front() {
                    events.push_back(Err(err));
                }
                // drop connection if necessary
                if connection.should_drop() {
                    self.server_connection = None;
                    self.pre_connection_timestamp = None;
                    self.pre_connection_digest = None;
                    self.connection_state = AwaitingChallengeResponse;
                    events.push_back(Ok(Event::Disconnection));
                    return events; // exit early, we're disconnected, who cares?
                }
                // process replays
                connection.process_replays();
                // receive messages
                while let Some(message) = connection.get_incoming_message() {
                    events.push_back(Ok(Event::Message(message)));
                }
                // receive replica actions
                while let Some(action) = connection.get_incoming_replica_action() {
                    let event: Event<T> = {
                        match action {
//                            ReplicaAction::CreateObject(local_key) => {
//                                Event::CreateObject(local_key)
//                            }
//                            ReplicaAction::DeleteObject(local_key, replica) => {
//                                Event::DeleteObject(local_key, replica.clone())
//                            }
//                            ReplicaAction::UpdateObject(local_key) => {
//                                Event::UpdateObject(local_key)
//                            }
//                            ReplicaAction::AssignPawn(local_key) => Event::AssignPawn(local_key),
//                            ReplicaAction::UnassignPawn(local_key) => {
//                                Event::UnassignPawn(local_key)
//                            }
//                            ReplicaAction::ResetPawn(local_key) => Event::ResetPawn(local_key),
                            ReplicaAction::CreateEntity(local_key, component_list) => {
                                Event::CreateEntity(local_key, component_list)
                            }
                            ReplicaAction::DeleteEntity(local_key) => {
                                Event::DeleteEntity(local_key)
                            }
                            ReplicaAction::AssignPawnEntity(local_key) => {
                                Event::AssignPawnEntity(local_key)
                            }
                            ReplicaAction::UnassignPawnEntity(local_key) => {
                                Event::UnassignPawnEntity(local_key)
                            }
                            ReplicaAction::ResetPawnEntity(local_key) => {
                                Event::ResetPawnEntity(local_key)
                            }
                            ReplicaAction::AddComponent(entity_key, component_key) => {
                                Event::AddComponent(entity_key, component_key)
                            }
                            ReplicaAction::UpdateComponent(entity_key, component_key) => {
                                Event::UpdateComponent(entity_key, component_key)
                            }
                            ReplicaAction::RemoveComponent(
                                entity_key,
                                component_key,
                                component,
                            ) => {
                                Event::RemoveComponent(entity_key, component_key, component.clone())
                            }
                        }
                    };
                    events.push_back(Ok(event));
                }
                // receive replay command
                while let Some((pawn_key, command)) = connection.get_incoming_replay() {
                    match pawn_key {
                        PawnKey::Object(object_key) => {
//                            events.push_back(Ok(Event::ReplayCommand(
//                                object_key,
//                                command.borrow().copy_to_protocol(),
//                            )));
                        }
                        PawnKey::Entity(entity_key) => {
                            events.push_back(Ok(Event::ReplayCommandEntity(
                                entity_key,
                                command.borrow().copy_to_protocol(),
                            )));
                        }
                    }
                }
                // receive command
                while let Some((pawn_key, command)) = connection.get_incoming_command() {
                    match pawn_key {
                        PawnKey::Object(object_key) => {
//                            events.push_back(Ok(Event::NewCommand(
//                                object_key,
//                                command.borrow().copy_to_protocol(),
//                            )));
                        }
                        PawnKey::Entity(entity_key) => {
                            events.push_back(Ok(Event::NewCommandEntity(
                                entity_key,
                                command.borrow().copy_to_protocol(),
                            )));
                        }
                    }
                }
                // send heartbeats
                if connection.should_send_heartbeat() {
                    Client::internal_send_with_connection(
                        self.tick_manager.get_client_tick(),
                        &mut self.sender,
                        connection,
                        PacketType::Heartbeat,
                        Packet::empty(),
                    );
                }
                // send pings
                if connection.should_send_ping() {
                    let ping_payload = connection.get_ping_payload();
                    Client::internal_send_with_connection(
                        self.tick_manager.get_client_tick(),
                        &mut self.sender,
                        connection,
                        PacketType::Ping,
                        ping_payload,
                    );
                }
                // send a packet
                while let Some(payload) = connection
                    .get_outgoing_packet(self.tick_manager.get_client_tick(), &self.manifest)
                {
                    self.sender.send(Packet::new_raw(payload));
                    connection.mark_sent();
                }
                // update current tick
                // apply updates on tick boundary
                if connection.frame_begin(&self.manifest, &mut self.tick_manager) {
                    events.push_back(Ok(Event::Tick));
                }
            }
            None => {
                if self.handshake_timer.ringing() {
                    self.handshake_timer.reset();
                    self.send_handshake_packets();
                }
            }
        }

        events
    }

    /// Queues up an Message to be sent to the Server
    /// pub fn queue_message<T: ImplRef<U>>(
    pub fn send_message<U: ImplRef<T>>(&mut self, message_ref: &U, guaranteed_delivery: bool) {
        if let Some(connection) = &mut self.server_connection {
            let dyn_ref = message_ref.dyn_ref();
            connection.queue_message(&dyn_ref, guaranteed_delivery);
        }
    }

    /// Queues up a Pawn Object Command to be sent to the Server
//    pub fn send_object_command<U: ImplRef<T>>(
//        &mut self,
//        pawn_object_key: &LocalObjectKey,
//        command_ref: &U,
//    ) {
//        if let Some(connection) = &mut self.server_connection {
//            let dyn_ref = command_ref.dyn_ref();
//            connection.object_queue_command(pawn_object_key, &dyn_ref);
//        }
//    }

    /// Queues up a Pawn Entity Command to be sent to the Server
    pub fn send_entity_command<U: ImplRef<T>>(
        &mut self,
        pawn_entity_key: &LocalEntityKey,
        command_ref: &U,
    ) {
        if let Some(connection) = &mut self.server_connection {
            let dyn_ref = command_ref.dyn_ref();
            connection.entity_queue_command(pawn_entity_key, &dyn_ref);
        }
    }

    /// Get the address currently associated with the Server
    pub fn server_address(&self) -> SocketAddr {
        return self.server_address;
    }

    /// Return whether or not a connection has been established with the Server
    pub fn has_connection(&self) -> bool {
        return self.server_connection.is_some();
    }

    // objects

    /// Get a reference to an Object currently in scope for the Client, given
    /// that Object's Key
//    pub fn get_object(&self, key: &LocalObjectKey) -> Option<&T> {
//        if let Some(connection) = &self.server_connection {
//            return connection.get_object(key);
//        }
//        return None;
//    }

    /// Get whether or not the Object currently in scope for the Client, given
    /// that Object's Key
//    pub fn has_object(&self, key: &LocalObjectKey) -> bool {
//        if let Some(connection) = &self.server_connection {
//            return connection.has_object(key);
//        }
//        return false;
//    }

    /// Component-themed alias for `get_object`
    pub fn get_component(&self, key: &LocalComponentKey) -> Option<&T> {
        if let Some(connection) = &self.server_connection {
            return connection.get_object(key);
        }
        return None;
    }

    /// Get whether or not the Component currently in scope for the Client,
    /// given that Component's Key
    pub fn has_component(&self, key: &LocalComponentKey) -> bool {
        if let Some(connection) = &self.server_connection {
            return connection.has_component(key);
        }
        return false;
    }

    /// Return an iterator to the collection of keys to all Objects tracked
    /// by the Client
//    pub fn object_keys(&self) -> Option<Vec<LocalObjectKey>> {
//        if let Some(connection) = &self.server_connection {
//            return Some(connection.object_keys());
//        }
//        return None;
//    }

    /// Return an iterator to the collection of keys to all Components tracked
    /// by the Client
    pub fn component_keys(&self) -> Option<Vec<LocalComponentKey>> {
        if let Some(connection) = &self.server_connection {
            return Some(connection.component_keys());
        }
        return None;
    }

    // pawns

    /// Get a reference to a Pawn
    pub fn get_pawn(&self, key: &LocalObjectKey) -> Option<&T> {
        if let Some(connection) = &self.server_connection {
            return connection.get_pawn(key);
        }
        return None;
    }

    /// Get a reference to a Pawn, used for setting it's state
    pub fn get_pawn_mut(&mut self, key: &LocalObjectKey) -> Option<&T> {
        if let Some(connection) = self.server_connection.as_mut() {
            return connection.get_pawn_mut(key);
        }
        return None;
    }

    /// Return an iterator to the collection of keys to all Pawns tracked by
    /// the Client
    pub fn pawn_keys(&self) -> Option<Vec<LocalObjectKey>> {
        if let Some(connection) = &self.server_connection {
            return Some(
                connection
                    .pawn_keys()
                    .cloned()
                    .collect::<Vec<LocalObjectKey>>(),
            );
        }
        return None;
    }

    // entities

    /// Get whether or not the Entity currently in scope for the Client, given
    /// that Entity's Key
    pub fn has_entity(&self, key: &LocalEntityKey) -> bool {
        if let Some(connection) = &self.server_connection {
            return connection.has_entity(key);
        }
        return false;
    }

    /// Get a set of Components for the Entity associated with the given
    /// EntityKey
    pub fn get_components(&self, key: &LocalEntityKey) -> Vec<T> {
        if let Some(connection) = &self.server_connection {
            return connection.get_components(key);
        }
        return Vec::<T>::new();
    }

    /// Get a set of Components for the Pawn Entity associated with the given
    /// EntityKey
    pub fn get_pawn_components(&self, key: &LocalEntityKey) -> Vec<T> {
        if let Some(connection) = &self.server_connection {
            return connection.get_pawn_components(key);
        }
        return Vec::<T>::new();
    }

    // connection metrics

    /// Gets the average Round Trip Time measured to the Server
    pub fn get_rtt(&self) -> f32 {
        return self.server_connection.as_ref().unwrap().get_rtt();
    }

    /// Gets the average Jitter measured in connection to the Server
    pub fn get_jitter(&self) -> f32 {
        return self.server_connection.as_ref().unwrap().get_jitter();
    }

    // ticks

    /// Gets the current tick of the Client
    pub fn get_client_tick(&self) -> u16 {
        return self.tick_manager.get_client_tick();
    }

    /// Gets the last received tick from the Server
    pub fn get_server_tick(&self) -> u16 {
        return self
            .server_connection
            .as_ref()
            .unwrap()
            .get_last_received_tick();
    }

    // interpolation

    /// Gets the interpolation tween amount for the current frame
    pub fn get_interpolation(&self) -> f32 {
        self.tick_manager.fraction
    }

    // internal functions

    fn send_handshake_packets(&mut self) {
        match self.connection_state {
            ConnectionState::Connected => {
                // do nothing, not necessary
            }
            ConnectionState::AwaitingChallengeResponse => {
                if self.pre_connection_timestamp.is_none() {
                    self.pre_connection_timestamp = Some(Timestamp::now());
                }

                let mut timestamp_bytes = Vec::new();
                self.pre_connection_timestamp
                    .as_mut()
                    .unwrap()
                    .write(&mut timestamp_bytes);
                Client::<T>::internal_send_connectionless(
                    &mut self.sender,
                    PacketType::ClientChallengeRequest,
                    Packet::new(timestamp_bytes),
                );
            }
            ConnectionState::AwaitingConnectResponse => {
                // write timestamp & digest into payload
                let mut payload_bytes = Vec::new();
                self.pre_connection_timestamp
                    .as_mut()
                    .unwrap()
                    .write(&mut payload_bytes);
                for digest_byte in self.pre_connection_digest.as_ref().unwrap().as_ref() {
                    payload_bytes.push(*digest_byte);
                }
                // write auth message if there is one
                if let Some(auth_message) = &mut self.auth_message {
                    let type_id = auth_message.borrow().get_type_id();
                    let naia_id = self.manifest.get_naia_id(&type_id); // get naia id
                    payload_bytes.write_u16::<BigEndian>(naia_id).unwrap(); // write naia id
                    auth_message.borrow().write(&mut payload_bytes);
                }
                Client::<T>::internal_send_connectionless(
                    &mut self.sender,
                    PacketType::ClientConnectRequest,
                    Packet::new(payload_bytes),
                );
            }
        }
    }

    fn maintain_socket(&mut self) {
        // receive from socket
        loop {
            match self.receiver.receive() {
                Ok(event) => {
                    if let Some(packet) = event {
                        let server_connection_wrapper = self.server_connection.as_mut();

                        if let Some(server_connection) = server_connection_wrapper {
                            server_connection.mark_heard();

                            let (header, payload) = StandardHeader::read(packet.payload());
                            server_connection
                                .process_incoming_header(&header, &mut self.tick_manager);

                            match header.packet_type() {
                                PacketType::Data => {
                                    server_connection.buffer_data_packet(
                                        header.host_tick(),
                                        header.local_packet_index(),
                                        &payload,
                                    );
                                }
                                PacketType::Heartbeat => {}
                                PacketType::Pong => {
                                    server_connection.process_pong(&payload);
                                }
                                _ => {} // TODO: explicitly cover these cases
                            }
                        } else {
                            let (header, payload) = StandardHeader::read(packet.payload());
                            match header.packet_type() {
                                PacketType::ServerChallengeResponse => {
                                    if self.connection_state
                                        == ConnectionState::AwaitingChallengeResponse
                                    {
                                        if let Some(my_timestamp) = self.pre_connection_timestamp {
                                            let mut reader = PacketReader::new(&payload);
                                            let server_tick = reader
                                                .get_cursor()
                                                .read_u16::<BigEndian>()
                                                .unwrap();
                                            let payload_timestamp = Timestamp::read(&mut reader);

                                            if my_timestamp == payload_timestamp {
                                                let mut digest_bytes: Vec<u8> = Vec::new();
                                                for _ in 0..32 {
                                                    digest_bytes.push(reader.read_u8());
                                                }
                                                self.pre_connection_digest =
                                                    Some(digest_bytes.into_boxed_slice());

                                                self.tick_manager.set_initial_tick(server_tick);

                                                self.connection_state =
                                                    ConnectionState::AwaitingConnectResponse;
                                            }
                                        }
                                    }
                                }
                                PacketType::ServerConnectResponse => {
                                    let server_connection = ServerConnection::new(
                                        self.server_address,
                                        &self.connection_config,
                                    );

                                    self.server_connection = Some(server_connection);
                                    self.connection_state = ConnectionState::Connected;
                                    self.outstanding_connect = true;
                                }
                                _ => {}
                            }
                        }
                    } else {
                        break;
                    }
                }
                Err(error) => {
                    self.outstanding_errors
                        .push_back(NaiaClientError::Wrapped(Box::new(error)));
                }
            }
        }
    }

    fn internal_send_with_connection(
        host_tick: u16,
        sender: &mut PacketSender,
        connection: &mut ServerConnection<T>,
        packet_type: PacketType,
        packet: Packet,
    ) {
        let new_payload = connection.process_outgoing_header(
            host_tick,
            connection.get_last_received_tick(),
            packet_type,
            packet.payload(),
        );
        sender.send(Packet::new_raw(new_payload));
        connection.mark_sent();
    }

    fn internal_send_connectionless(
        sender: &mut PacketSender,
        packet_type: PacketType,
        packet: Packet,
    ) {
        let new_payload =
            naia_shared::utils::write_connectionless_payload(packet_type, packet.payload());
        sender.send(Packet::new_raw(new_payload));
    }
}
