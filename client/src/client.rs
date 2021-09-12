use std::{collections::VecDeque, net::SocketAddr};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use naia_client_socket::{ClientSocket, PacketReceiver, PacketSender};

pub use naia_shared::{
    ConnectionConfig, HostTickManager, ImplRef, Instant, LocalEntityKey,
    ManagerType, Manifest, PacketReader, PacketType, ProtocolType, Ref, Replicate,
    SequenceIterator, SharedConfig, StandardHeader, Timer, Timestamp,
};

use super::{
    client_config::ClientConfig,
    connection_state::{ConnectionState, ConnectionState::AwaitingChallengeResponse},
    entity_action::EntityAction,
    error::NaiaClientError,
    event::Event,
    server_connection::ServerConnection,
    tick_manager::TickManager,
    Packet,
};

/// Client can send/receive messages to/from a server, and has a pool of
/// in-scope entities/components that are synced with the server
#[derive(Debug)]
pub struct Client<T: ProtocolType> {
    manifest: Manifest<T>,
    // Connection
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
    // Events
    outstanding_connect: bool,
    outstanding_errors: VecDeque<NaiaClientError>,
    // Ticks
    tick_manager: TickManager,
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
            manifest,
            // Connection
            server_address,
            sender,
            receiver,
            connection_config,
            handshake_timer,
            server_connection: None,
            pre_connection_timestamp: None,
            pre_connection_digest: None,
            connection_state: AwaitingChallengeResponse,
            auth_message,
            // Events
            outstanding_connect: false,
            outstanding_errors: VecDeque::new(),
            // Ticks
            tick_manager: TickManager::new(shared_config.tick_interval),
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
                // receive entity actions
                while let Some(action) = connection.get_incoming_entity_action() {
                    let event: Event<T> = {
                        match action {
                            EntityAction::CreateEntity(local_key, component_list) => {
                                Event::CreateEntity(local_key, component_list)
                            }
                            EntityAction::DeleteEntity(local_key) => Event::DeleteEntity(local_key),
                            EntityAction::AssignPawn(local_key) => {
                                Event::AssignPawnEntity(local_key)
                            }
                            EntityAction::UnassignPawn(local_key) => Event::UnassignPawn(local_key),
                            EntityAction::ResetPawn(local_key) => Event::ResetPawn(local_key),
                            EntityAction::AddComponent(entity_key, component_key) => {
                                Event::AddComponent(entity_key, component_key)
                            }
                            EntityAction::UpdateComponent(entity_key, component_key) => {
                                Event::UpdateComponent(entity_key, component_key)
                            }
                            EntityAction::RemoveComponent(entity_key, component) => {
                                Event::RemoveComponent(entity_key, component.clone())
                            }
                        }
                    };
                    events.push_back(Ok(event));
                }
                // receive replay command
                while let Some((pawn_key, command)) = connection.get_incoming_replay() {
                    events.push_back(Ok(Event::ReplayCommandEntity(
                        pawn_key,
                        command.borrow().copy_to_protocol(),
                    )));
                }
                // receive command
                while let Some((pawn_key, command)) = connection.get_incoming_command() {
                    events.push_back(Ok(Event::NewCommandEntity(
                        pawn_key,
                        command.borrow().copy_to_protocol(),
                    )));
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

    // Messages

    /// Queues up an Message to be sent to the Server
    /// pub fn queue_message<T: ImplRef<U>>(
    pub fn queue_message<U: ImplRef<T>>(&mut self, message_ref: &U, guaranteed_delivery: bool) {
        if let Some(connection) = &mut self.server_connection {
            let dyn_ref = message_ref.dyn_ref();
            connection.queue_message(&dyn_ref, guaranteed_delivery);
        }
    }

    /// Queues up a Pawn Command to be sent to the Server
    pub fn queue_command<U: ImplRef<T>>(&mut self, entity_key: &LocalEntityKey, command_ref: &U) {
        if let Some(connection) = &mut self.server_connection {
            let dyn_ref = command_ref.dyn_ref();
            connection.queue_command(entity_key, &dyn_ref);
        }
    }

    // Entities

    /// Get whether or not the Entity currently in scope for the Client, given
    /// that Entity's Key
    pub fn entity_exists(&self, key: &LocalEntityKey) -> bool {
        if let Some(connection) = &self.server_connection {
            return connection.has_entity(key);
        }
        return false;
    }

    /// Get whether or not the Entity associated with a given EntityKey has
    /// been assigned as a Pawn
    pub fn entity_is_pawn(&self, key: &LocalEntityKey) -> bool {
        if let Some(connection) = &self.server_connection {
            return connection.entity_is_pawn(key);
        }
        return false;
    }

    // Components

    /// Given an EntityKey & a Component type, get a reference to the appropriate ComponentRef
    pub fn component_past<R: Replicate<T>>(&self, entity_key: &LocalEntityKey) -> Option<&Ref<R>> {
        if let Some(protocol) = self.get_component_by_type::<R>(entity_key) {
            return protocol.as_typed_ref::<R>();
        }
        return None;
    }

    /// Given an EntityKey & a Component type, get a reference to the appropriate ComponentRef
    pub fn component_present<R: Replicate<T>>(&self, entity_key: &LocalEntityKey) -> Option<&Ref<R>> {
        if let Some(protocol) = self.get_pawn_component_by_type::<R>(entity_key) {
            return protocol.as_typed_ref::<R>();
        }
        return None;
    }

//    /// Get a set of Components for the Entity associated with the given
//    /// EntityKey
//    pub fn get_components(&self, key: &LocalEntityKey) -> Vec<T> {
//        if let Some(connection) = &self.server_connection {
//            return connection.get_components(key);
//        }
//        return Vec::<T>::new();
//    }
//
//    /// Get a set of Components for the Pawn Entity associated with the given
//    /// EntityKey
//    pub fn get_pawn_components(&self, key: &LocalEntityKey) -> Vec<T> {
//        if let Some(connection) = &self.server_connection {
//            return connection.get_pawn_components(key);
//        }
//        return Vec::<T>::new();
//    }

    // Connection

    /// Get the address currently associated with the Server
    pub fn server_address(&self) -> SocketAddr {
        return self.server_address;
    }

    /// Return whether or not a connection has been established with the Server
    pub fn connected(&self) -> bool {
        return self.server_connection.is_some();
    }

    /// Gets the average Round Trip Time measured to the Server
    pub fn rtt(&self) -> f32 {
        return self.server_connection.as_ref().unwrap().get_rtt();
    }

    /// Gets the average Jitter measured in connection to the Server
    pub fn jitter(&self) -> f32 {
        return self.server_connection.as_ref().unwrap().get_jitter();
    }

    // Ticks

    /// Gets the current tick of the Client
    pub fn client_tick(&self) -> u16 {
        return self.tick_manager.get_client_tick();
    }

    /// Gets the last received tick from the Server
    pub fn server_tick(&self) -> u16 {
        return self
            .server_connection
            .as_ref()
            .unwrap()
            .get_last_received_tick();
    }

    // Interpolation

    /// Gets the interpolation tween amount for the current frame
    pub fn interpolation(&self) -> f32 {
        self.tick_manager.fraction
    }

    // Crate-Public functions

    /// Given an EntityKey & a Component type, get a reference to a registered
    /// Component being tracked by the Server
    pub(crate) fn get_component_by_type<R: Replicate<T>>(&self, key: &LocalEntityKey) -> Option<&T> {
        if let Some(connection) = &self.server_connection {
            return connection.get_component_by_type::<R>(key);
        }
        return None;
    }

    /// Given an EntityKey & a Component type, get a reference to a registered
    /// Pawn Component being tracked by the Server
    pub(crate) fn get_pawn_component_by_type<R: Replicate<T>>(
        &self,
        key: &LocalEntityKey,
    ) -> Option<&T> {
        if let Some(connection) = &self.server_connection {
            return connection.get_pawn_component_by_type::<R>(key);
        }
        return None;
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
