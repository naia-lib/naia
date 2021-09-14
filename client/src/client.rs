use std::{collections::VecDeque, net::SocketAddr};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use naia_client_socket::{NaiaClientSocketError, PacketReceiver, PacketSender, Socket};

pub use naia_shared::{
    ConnectionConfig, HostTickManager, ImplRef, LocalEntityKey, ManagerType, Manifest,
    PacketReader, PacketType, ProtocolType, Ref, Replicate, SequenceIterator, SharedConfig,
    StandardHeader, Timer, Timestamp,
};

use super::{
    client_config::ClientConfig,
    connection_state::{ConnectionState, ConnectionState::AwaitingChallengeResponse},
    entity_action::EntityAction,
    entity_ref::EntityRef,
    error::NaiaClientError,
    event::Event,
    server_connection::ServerConnection,
    tick_manager::TickManager,
    Packet,
};

/// Client can send/receive messages to/from a server, and has a pool of
/// in-scope entities/components that are synced with the server
#[derive(Debug)]
pub struct Client<P: ProtocolType> {
    // Manifest
    manifest: Manifest<P>,
    // Connection
    connection_config: ConnectionConfig,
    socket: Socket,
    io: Io,
    address: Option<SocketAddr>,
    server_connection: Option<ServerConnection<P>>,
    pre_connection_timestamp: Option<Timestamp>,
    pre_connection_digest: Option<Box<[u8]>>,
    handshake_timer: Timer,
    connection_state: ConnectionState,
    auth_message: Option<Ref<dyn Replicate<P>>>,
    // Events
    outstanding_connect: bool,
    outstanding_errors: VecDeque<NaiaClientError>,
    // Ticks
    tick_manager: TickManager,
}

impl<P: ProtocolType> Client<P> {
    /// Create a new Client
    pub fn new(mut client_config: ClientConfig, shared_config: SharedConfig<P>) -> Self {
        client_config.socket_config.link_condition_config =
            shared_config.link_condition_config.clone();

        let connection_config = ConnectionConfig::new(
            client_config.disconnection_timeout_duration,
            client_config.heartbeat_interval,
            client_config.ping_interval,
            client_config.rtt_sample_size,
        );

        let socket = Socket::new(client_config.socket_config);

        let mut handshake_timer = Timer::new(client_config.send_handshake_interval);
        handshake_timer.ring_manual();

        Client {
            // Manifest
            manifest: shared_config.manifest,
            // Connection
            io: Io::new(),
            socket,
            connection_config,
            address: None,
            handshake_timer,
            server_connection: None,
            pre_connection_timestamp: None,
            pre_connection_digest: None,
            connection_state: AwaitingChallengeResponse,
            auth_message: None,
            // Events
            outstanding_connect: false,
            outstanding_errors: VecDeque::new(),
            // Ticks
            tick_manager: TickManager::new(shared_config.tick_interval),
        }
    }

    /// Connect to the given server address
    pub fn connect<R: ImplRef<P>>(&mut self, server_address: SocketAddr, auth: Option<R>) {
        self.address = Some(server_address);
        self.socket.connect(server_address);
        self.io.load(
            self.socket.get_packet_sender(),
            self.socket.get_packet_receiver(),
        );

        self.auth_message = {
            if auth.is_none() {
                None
            } else {
                Some(auth.unwrap().dyn_ref())
            }
        };
    }

    // Messages

    /// Queues up an Message to be sent to the Server
    pub fn queue_message<R: ImplRef<P>>(&mut self, message_ref: &R, guaranteed_delivery: bool) {
        if let Some(connection) = &mut self.server_connection {
            let dyn_ref = message_ref.dyn_ref();
            connection.queue_message(&dyn_ref, guaranteed_delivery);
        }
    }

    /// Queues up a Command for an assigned Entity to be sent to the Server
    pub fn queue_command<R: ImplRef<P>>(&mut self, entity_key: &LocalEntityKey, command_ref: &R) {
        if let Some(connection) = &mut self.server_connection {
            let dyn_ref = command_ref.dyn_ref();
            connection.queue_command(entity_key, &dyn_ref);
        }
    }

    // Entities

    /// Retrieves an EntityRef that exposes read-only operations for the
    /// Entity associated with the given LocalEntityKey.
    /// Panics if the Entity does not exist for the given LocalEntityKey.
    pub fn entity(&self, entity_key: &LocalEntityKey) -> EntityRef<P> {
        if self.entity_exists(entity_key) {
            return EntityRef::new(self, &entity_key);
        }
        panic!("No Entity exists for given Key!");
    }

    /// Get whether or not the Entity currently in scope for the Client, given
    /// that Entity's Key
    pub fn entity_exists(&self, entity_key: &LocalEntityKey) -> bool {
        if let Some(connection) = &self.server_connection {
            return connection.has_entity(entity_key);
        }
        return false;
    }

    // Connection

    /// Get the address currently associated with the Server
    pub fn server_address(&self) -> SocketAddr {
        return self
            .address
            .expect("Client has not initiated connection to Server yet!");
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

    // Receive Data from Server! Very important!

    /// Must call this regularly (preferably at the beginning of every draw
    /// frame), in a loop until it returns None.
    /// Retrieves incoming update data, and maintains the connection.
    pub fn receive(&mut self) -> VecDeque<Result<Event<P>, NaiaClientError>> {
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
                    let event: Event<P> = {
                        match action {
                            EntityAction::SpawnEntity(local_key, component_list) => {
                                Event::SpawnEntity(local_key, component_list)
                            }
                            EntityAction::DespawnEntity(local_key) => {
                                Event::DespawnEntity(local_key)
                            }
                            EntityAction::OwnEntity(local_key) => Event::OwnEntity(local_key),
                            EntityAction::DisownEntity(local_key) => Event::DisownEntity(local_key),
                            EntityAction::RewindEntity(local_key) => Event::RewindEntity(local_key),
                            EntityAction::InsertComponent(entity_key, component_key) => {
                                Event::InsertComponent(entity_key, component_key)
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
                while let Some((prediction_key, command)) = connection.get_incoming_replay() {
                    events.push_back(Ok(Event::ReplayCommand(
                        prediction_key,
                        command.borrow().copy_to_protocol(),
                    )));
                }
                // receive command
                while let Some((prediction_key, command)) = connection.get_incoming_command() {
                    events.push_back(Ok(Event::NewCommand(
                        prediction_key,
                        command.borrow().copy_to_protocol(),
                    )));
                }
                // send heartbeats
                if connection.should_send_heartbeat() {
                    Client::internal_send_with_connection(
                        self.tick_manager.get_client_tick(),
                        &mut self.io,
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
                        &mut self.io,
                        connection,
                        PacketType::Ping,
                        ping_payload,
                    );
                }
                // send a packet
                while let Some(payload) = connection
                    .get_outgoing_packet(self.tick_manager.get_client_tick(), &self.manifest)
                {
                    self.io.send_packet(Packet::new_raw(payload));
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

    // Crate-Public functions

    //// Entities

    /// Get whether or not the Entity associated with a given EntityKey has
    /// been assigned to the current User
    pub(crate) fn entity_is_owned(&self, entity_key: &LocalEntityKey) -> bool {
        if let Some(connection) = &self.server_connection {
            return connection.entity_is_prediction(entity_key);
        }
        return false;
    }

    /// Returns whether or not an Entity has a Component of a given TypeId
    pub(crate) fn entity_contains_type<R: Replicate<P>>(
        &self,
        entity_key: &LocalEntityKey,
    ) -> bool {
        return self.get_component_by_type::<R>(entity_key).is_some();
    }

    //// Components

    /// Given an EntityKey & a Component type, get a reference to the
    /// appropriate ComponentRef
    pub(crate) fn component<R: Replicate<P>>(
        &self,
        entity_key: &LocalEntityKey,
    ) -> Option<&Ref<R>> {
        if let Some(protocol) = self.get_component_by_type::<R>(entity_key) {
            return protocol.as_typed_ref::<R>();
        }
        return None;
    }

    /// Given an EntityKey & a Component type, get a reference to the
    /// appropriate ComponentRef
    pub(crate) fn component_prediction<R: Replicate<P>>(
        &self,
        entity_key: &LocalEntityKey,
    ) -> Option<&Ref<R>> {
        if let Some(protocol) = self.get_prediction_component_by_type::<R>(entity_key) {
            return protocol.as_typed_ref::<R>();
        }
        return None;
    }

    /// Given an EntityKey & a Component type, get a reference to a registered
    /// Component being tracked by the Server
    pub(crate) fn get_component_by_type<R: Replicate<P>>(
        &self,
        entity_key: &LocalEntityKey,
    ) -> Option<&P> {
        if let Some(connection) = &self.server_connection {
            return connection.get_component_by_type::<R>(entity_key);
        }
        return None;
    }

    /// Given an EntityKey & a Component type, get a reference to a registered
    /// Prediction Component being tracked by the Server
    pub(crate) fn get_prediction_component_by_type<R: Replicate<P>>(
        &self,
        entity_key: &LocalEntityKey,
    ) -> Option<&P> {
        if let Some(connection) = &self.server_connection {
            return connection.get_prediction_component_by_type::<R>(entity_key);
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
                Client::<P>::internal_send_connectionless(
                    &mut self.io,
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
                Client::<P>::internal_send_connectionless(
                    &mut self.io,
                    PacketType::ClientConnectRequest,
                    Packet::new(payload_bytes),
                );
            }
        }
    }

    fn maintain_socket(&mut self) {
        // receive from socket
        loop {
            match self.io.receive_packet() {
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
                                        self.server_address(),
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
        io: &mut Io,
        connection: &mut ServerConnection<P>,
        packet_type: PacketType,
        packet: Packet,
    ) {
        let new_payload = connection.process_outgoing_header(
            host_tick,
            connection.get_last_received_tick(),
            packet_type,
            packet.payload(),
        );
        io.send_packet(Packet::new_raw(new_payload));
        connection.mark_sent();
    }

    fn internal_send_connectionless(io: &mut Io, packet_type: PacketType, packet: Packet) {
        let new_payload =
            naia_shared::utils::write_connectionless_payload(packet_type, packet.payload());
        io.send_packet(Packet::new_raw(new_payload));
    }
}

// IO
#[derive(Debug)]
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

    pub fn send_packet(&mut self, packet: Packet) {
        self.packet_sender
            .as_mut()
            .expect("Cannot call Client.send_packet() until you call Client.connect()!")
            .send(packet);
    }

    pub fn receive_packet(&mut self) -> Result<Option<Packet>, NaiaClientSocketError> {
        return self
            .packet_receiver
            .as_mut()
            .expect("Cannot call Client.receive_packet() until you call Client.connect()!")
            .receive();
    }
}
