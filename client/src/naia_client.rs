use std::net::SocketAddr;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use naia_client_socket::{ClientSocket, ClientSocketTrait, MessageSender};

pub use naia_shared::{
    ActorType, ConnectionConfig, Event, EventType, HostTickManager, Instant, LocalActorKey,
    ManagerType, Manifest, PacketReader, PacketType, SequenceIterator, SharedConfig,
    StandardHeader, Timer, Timestamp,
};

use super::{
    client_actor_message::ClientActorMessage, client_config::ClientConfig,
    client_event::ClientEvent, client_tick_manager::ClientTickManager, error::NaiaClientError,
    server_connection::ServerConnection, Packet,
};
use crate::client_connection_state::{
    ClientConnectionState, ClientConnectionState::AwaitingChallengeResponse,
};

/// Client can send/receive events to/from a server, and has a pool of in-scope
/// actors that are synced with the server
#[derive(Debug)]
pub struct NaiaClient<T: EventType, U: ActorType> {
    manifest: Manifest<T, U>,
    server_address: SocketAddr,
    connection_config: ConnectionConfig,
    socket: Box<dyn ClientSocketTrait>,
    sender: MessageSender,
    server_connection: Option<ServerConnection<T, U>>,
    pre_connection_timestamp: Option<Timestamp>,
    pre_connection_digest: Option<Box<[u8]>>,
    handshake_timer: Timer,
    connection_state: ClientConnectionState,
    auth_event: Option<T>,
    tick_manager: ClientTickManager,
}

impl<T: EventType, U: ActorType> NaiaClient<T, U> {
    /// Create a new client, given the server's address, a shared manifest, an
    /// optional Config, and an optional Authentication event
    pub fn new(
        server_address: SocketAddr,
        manifest: Manifest<T, U>,
        client_config: Option<ClientConfig>,
        shared_config: SharedConfig,
        auth: Option<T>,
    ) -> Self {
        let client_config = match client_config {
            Some(config) => config,
            None => ClientConfig::default(),
        };

        let connection_config = ConnectionConfig::new(
            client_config.disconnection_timeout_duration,
            client_config.heartbeat_interval,
            client_config.ping_interval,
            client_config.rtt_sample_size,
        );

        let mut client_socket = ClientSocket::connect(server_address);
        if let Some(config) = shared_config.link_condition_config {
            client_socket = client_socket.with_link_conditioner(&config);
        }

        let mut handshake_timer = Timer::new(client_config.send_handshake_interval);
        handshake_timer.ring_manual();
        let message_sender = client_socket.get_sender();

        NaiaClient {
            server_address,
            manifest,
            socket: client_socket,
            sender: message_sender,
            connection_config,
            handshake_timer,
            server_connection: None,
            pre_connection_timestamp: None,
            pre_connection_digest: None,
            connection_state: AwaitingChallengeResponse,
            auth_event: auth,
            tick_manager: ClientTickManager::new(shared_config.tick_interval),
        }
    }

    /// Must call this regularly (preferably at the beginning of every draw
    /// frame), in a loop until it returns None.
    /// Retrieves incoming events/updates, and performs updates to maintain the
    /// connection.
    pub fn receive(&mut self) -> Option<Result<ClientEvent<T>, NaiaClientError>> {
        // send ticks, handshakes, heartbeats, pings, timeout if need be
        match &mut self.server_connection {
            Some(connection) => {
                // receive command
                if let Some((pawn_key, command)) = connection.get_incoming_command() {
                    return Some(Ok(ClientEvent::Command(
                        pawn_key,
                        command.as_ref().get_typed_copy(),
                    )));
                }
                // receive event
                if let Some(event) = connection.get_incoming_event() {
                    return Some(Ok(ClientEvent::Event(event)));
                }
                // receive actor message
                if let Some(message) = connection.get_incoming_actor_message() {
                    match message {
                        ClientActorMessage::Create(local_key) => {
                            return Some(Ok(ClientEvent::CreateActor(local_key)));
                        }
                        ClientActorMessage::Delete(local_key) => {
                            return Some(Ok(ClientEvent::DeleteActor(local_key)));
                        }
                        ClientActorMessage::Update(local_key) => {
                            return Some(Ok(ClientEvent::UpdateActor(local_key)));
                        }
                        ClientActorMessage::AssignPawn(local_key) => {
                            return Some(Ok(ClientEvent::AssignPawn(local_key)));
                        }
                        ClientActorMessage::UnassignPawn(local_key) => {
                            return Some(Ok(ClientEvent::UnassignPawn(local_key)));
                        }
                    }
                }
                // update current tick
                if self.tick_manager.take_tick() {
                    return Some(Ok(ClientEvent::Tick));
                }
                // drop connection if necessary
                if connection.should_drop() {
                    self.server_connection = None;
                    self.pre_connection_timestamp = None;
                    self.pre_connection_digest = None;
                    self.connection_state = AwaitingChallengeResponse;
                    return Some(Ok(ClientEvent::Disconnection));
                } else {
                    // send heartbeats
                    if connection.should_send_heartbeat() {
                        NaiaClient::internal_send_with_connection(
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
                        NaiaClient::internal_send_with_connection(
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
                        self.sender
                            .send(Packet::new_raw(payload))
                            .expect("send failed!");
                        connection.mark_sent();
                    }
                }
            }
            None => {
                if self.handshake_timer.ringing() {
                    match self.connection_state {
                        ClientConnectionState::AwaitingChallengeResponse => {
                            if self.pre_connection_timestamp.is_none() {
                                self.pre_connection_timestamp = Some(Timestamp::now());
                            }

                            let mut timestamp_bytes = Vec::new();
                            self.pre_connection_timestamp
                                .as_mut()
                                .unwrap()
                                .write(&mut timestamp_bytes);
                            NaiaClient::<T, U>::internal_send_connectionless(
                                &mut self.sender,
                                PacketType::ClientChallengeRequest,
                                Packet::new(timestamp_bytes),
                            );
                        }
                        ClientConnectionState::AwaitingConnectResponse => {
                            // write timestamp & digest into payload
                            let mut payload_bytes = Vec::new();
                            self.pre_connection_timestamp
                                .as_mut()
                                .unwrap()
                                .write(&mut payload_bytes);
                            for digest_byte in self.pre_connection_digest.as_ref().unwrap().as_ref()
                            {
                                payload_bytes.push(*digest_byte);
                            }
                            // write auth event object if there is one
                            if let Some(auth_event) = &mut self.auth_event {
                                let type_id = auth_event.get_type_id();
                                let naia_id = self.manifest.get_event_naia_id(&type_id); // get naia id
                                payload_bytes.write_u16::<BigEndian>(naia_id).unwrap(); // write naia id
                                auth_event.write(&mut payload_bytes);
                            }
                            NaiaClient::<T, U>::internal_send_connectionless(
                                &mut self.sender,
                                PacketType::ClientConnectRequest,
                                Packet::new(payload_bytes),
                            );
                        }
                        _ => {}
                    }

                    self.handshake_timer.reset();
                }
            }
        }

        // receive from socket
        loop {
            match self.socket.receive() {
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
                                    continue;
                                }
                                PacketType::Heartbeat => {
                                    continue;
                                }
                                PacketType::Pong => {
                                    server_connection.process_pong(&payload);
                                    continue;
                                }
                                _ => {}
                            }
                        } else {
                            let (header, payload) = StandardHeader::read(packet.payload());
                            match header.packet_type() {
                                PacketType::ServerChallengeResponse => {
                                    if self.connection_state
                                        == ClientConnectionState::AwaitingChallengeResponse
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
                                                    ClientConnectionState::AwaitingConnectResponse;
                                            }
                                        }
                                    }

                                    continue;
                                }
                                PacketType::ServerConnectResponse => {
                                    let server_connection = ServerConnection::new(
                                        self.server_address,
                                        &self.connection_config,
                                        &self.tick_manager,
                                    );

                                    self.server_connection = Some(server_connection);
                                    self.connection_state = ClientConnectionState::Connected;
                                    return Some(Ok(ClientEvent::Connection));
                                }
                                _ => {}
                            }
                        }
                    } else {
                        break;
                    }
                }
                Err(error) => {
                    return Some(Err(NaiaClientError::Wrapped(Box::new(error))));
                }
            }
        }

        // apply updates on tick boundary, and interpolate
        if let Some(connection) = &mut self.server_connection {
            connection.frame_begin(&self.manifest, &mut self.tick_manager);
        }

        return None;
    }

    /// Queues up an Event to be sent to the Server
    pub fn send_event(&mut self, event: &impl Event<T>) {
        if let Some(connection) = &mut self.server_connection {
            connection.queue_event(event);
        }
    }

    /// Queues up an Command to be sent to the Server
    pub fn send_command(&mut self, pawn_key: LocalActorKey, command: &impl Event<T>) {
        if let Some(connection) = &mut self.server_connection {
            connection.queue_command(pawn_key, command);
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

    // actors

    /// Get a reference to an Actor currently in scope for the Client, given
    /// that Actor's Key
    pub fn get_actor(&mut self, key: &LocalActorKey) -> Option<&U> {
        return self
            .server_connection
            .as_mut()
            .unwrap()
            .get_actor(&self.tick_manager, key);
    }

    /// Return an iterator to the collection of keys to all actors tracked by
    /// the Client
    pub fn actor_keys(&self) -> Option<Vec<LocalActorKey>> {
        if let Some(connection) = &self.server_connection {
            return Some(
                connection
                    .actor_keys()
                    .cloned()
                    .collect::<Vec<LocalActorKey>>(),
            );
        }
        return None;
    }

    // pawns

    /// Get a reference to a Pawn
    pub fn get_pawn(&mut self, key: &LocalActorKey) -> Option<&U> {
        return self
            .server_connection
            .as_mut()
            .unwrap()
            .get_pawn(&self.tick_manager, key);
    }

    /// Get a reference to a Pawn, used for setting it's state
    pub fn get_pawn_mut(&mut self, key: &LocalActorKey) -> Option<&U> {
        return self.server_connection.as_mut().unwrap().get_pawn_mut(key);
    }

    /// Return an iterator to the collection of keys to all Pawns tracked by
    /// the Client
    pub fn pawn_keys(&self) -> Option<Vec<LocalActorKey>> {
        if let Some(connection) = &self.server_connection {
            return Some(
                connection
                    .pawn_keys()
                    .cloned()
                    .collect::<Vec<LocalActorKey>>(),
            );
        }
        return None;
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

    // internal functions

    fn internal_send_with_connection(
        host_tick: u16,
        sender: &mut MessageSender,
        connection: &mut ServerConnection<T, U>,
        packet_type: PacketType,
        packet: Packet,
    ) {
        let new_payload = connection.process_outgoing_header(
            host_tick,
            connection.get_last_received_tick(),
            packet_type,
            packet.payload(),
        );
        sender
            .send(Packet::new_raw(new_payload))
            .expect("send failed!");
        connection.mark_sent();
    }

    fn internal_send_connectionless(
        sender: &mut MessageSender,
        packet_type: PacketType,
        packet: Packet,
    ) {
        let new_payload =
            naia_shared::utils::write_connectionless_payload(packet_type, packet.payload());
        sender
            .send(Packet::new_raw(new_payload))
            .expect("send failed!");
    }
}
