
use std::{
    net::SocketAddr,
};

use log::info;
use byteorder::{BigEndian, WriteBytesExt};

use naia_client_socket::{ClientSocket, SocketEvent, MessageSender, Config as SocketConfig};
pub use naia_shared::{Config, LocalEntityKey, PacketType, Timer, Timestamp,
                      Manifest, ManagerType, HostType, PacketWriter, PacketReader,
                      Event, EventType, EntityType};

use super::{
    server_connection::ServerConnection,
    client_event::ClientEvent,
    client_entity_message::ClientEntityMessage,
    error::NaiaClientError,
    Packet
};
use crate::client_connection_state::ClientConnectionState::AwaitingChallengeResponse;
use crate::client_connection_state::ClientConnectionState;

pub struct NaiaClient<T: EventType, U: EntityType> {
    manifest: Manifest<T, U>,
    server_address: SocketAddr,
    config: Config,
    socket: ClientSocket,
    sender: MessageSender,
    server_connection: Option<ServerConnection<T, U>>,
    pre_connection_timestamp: Option<Timestamp>,
    pre_connection_digest: Option<Box<[u8]>>,
    handshake_timer: Timer,
    drop_counter: u8,
    drop_max: u8,
    connection_state: ClientConnectionState,
    auth_event: Option<T>,
}

impl<T: EventType, U: EntityType> NaiaClient<T, U> {
    pub fn new(server_address: &str, manifest: Manifest<T, U>, config: Option<Config>, auth: Option<T>) -> Self {

        let mut config = match config {
            Some(config) => config,
            None => Config::default()
        };
        config.heartbeat_interval /= 2;

        let socket_config = SocketConfig::default();
        let mut client_socket = ClientSocket::connect(&server_address, Some(socket_config));

        let mut handshake_timer = Timer::new(config.send_handshake_interval);
        handshake_timer.ring_manual();
        let message_sender = client_socket.get_sender();

        NaiaClient {
            server_address: server_address.parse().unwrap(),
            manifest,
            socket: client_socket,
            sender: message_sender,
            drop_counter: 1,
            drop_max: 2,
            config,
            handshake_timer,
            server_connection: None,
            pre_connection_timestamp: None,
            pre_connection_digest: None,
            connection_state: AwaitingChallengeResponse,
            auth_event: auth,
        }
    }

    pub fn receive(&mut self) -> Result<ClientEvent<T>, NaiaClientError> {

        // send handshakes, send heartbeats, timeout if need be
        match &mut self.server_connection {
            Some(connection) => {
                if connection.should_drop() {
                    self.server_connection = None;
                    self.pre_connection_timestamp = None;
                    self.pre_connection_digest = None;
                    self.connection_state = AwaitingChallengeResponse;
                    return Ok(ClientEvent::Disconnection);
                }
                if connection.should_send_heartbeat() {
                    NaiaClient::internal_send_with_connection(&mut self.sender, connection, PacketType::Heartbeat, Packet::empty());
                }
                // send a packet
                if let Some(payload) = connection.get_outgoing_packet(&self.manifest) {
                    self.sender.send(Packet::new_raw(payload))
                        .expect("send failed!");
                    connection.mark_sent();
                }
                // receive event
                if let Some(event) = connection.get_incoming_event() {
                    return Ok(ClientEvent::Event(event));
                }
                // receive entity message
                if let Some(message) = connection.get_incoming_entity_message() {
                    match message {
                        ClientEntityMessage::Create(local_key) => {
                            return Ok(ClientEvent::CreateEntity(local_key));
                        },
                        ClientEntityMessage::Delete(local_key) => {
                            return Ok(ClientEvent::DeleteEntity(local_key));
                        },
                        ClientEntityMessage::Update(local_key) => {
                            return Ok(ClientEvent::UpdateEntity(local_key));
                        }
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
                            self.pre_connection_timestamp.as_mut().unwrap().write(&mut timestamp_bytes);
                            NaiaClient::<T,U>::internal_send_connectionless(
                                &mut self.sender,
                                PacketType::ClientChallengeRequest,
                                Packet::new(timestamp_bytes));
                        }
                        ClientConnectionState::AwaitingConnectResponse => {

                            // write timestamp & digest into payload
                            let mut payload_bytes = Vec::new();
                            self.pre_connection_timestamp.as_mut().unwrap().write(&mut payload_bytes);
                            for digest_byte in self.pre_connection_digest.as_ref().unwrap().as_ref() {
                                payload_bytes.push(*digest_byte);
                            }
                            // write auth event object if there is one
                            if let Some(auth_event) = &mut self.auth_event {
                                let type_id = auth_event.get_type_id();
                                let naia_id = self.manifest.get_event_naia_id(&type_id); // get naia id
                                payload_bytes.write_u16::<BigEndian>(naia_id).unwrap();// write naia id
                                auth_event.write(&mut payload_bytes);
                            }
                            NaiaClient::<T,U>::internal_send_connectionless(
                                &mut self.sender,
                                PacketType::ClientConnectRequest,
                                Packet::new(payload_bytes));
                        }
                        _ => {}
                    }

                    self.handshake_timer.reset();
                }
            }
        }

        // receive from socket
        let mut output: Option<Result<ClientEvent<T>, NaiaClientError>> = None;
        while output.is_none() {
            match self.socket.receive() {
                Ok(event) => {
                    match event {
                        SocketEvent::Packet(packet) => {

                            let packet_type = PacketType::get_from_packet(packet.payload());

                            // simulate dropping data packets //
                            if packet_type == PacketType::Data {

                                if self.drop_counter >= self.drop_max {
                                    self.drop_counter = 0;
                                    info!("~~~~~~~~~~  dropped packet from server  ~~~~~~~~~~");
                                    continue;
                                } else {
                                    self.drop_counter += 1;
                                }
                            }
                            /////////////////////////////////////

                            let server_connection_wrapper = self.server_connection.as_mut();
                            if let Some(server_connection) = server_connection_wrapper {
                                server_connection.mark_heard();
                                let mut payload = server_connection.process_incoming_header(packet.payload());

                                match packet_type {
                                    PacketType::Data => {
                                        server_connection.process_incoming_data(&self.manifest, &mut payload);
                                        continue;
                                    }
                                    PacketType::Heartbeat => {
                                        info!("<- s");
                                        continue;
                                    }
                                    _ => {}
                                }
                            }
                            else {
                                match packet_type {
                                    PacketType::ServerChallengeResponse => {

                                        if self.connection_state == ClientConnectionState::AwaitingChallengeResponse {
                                            if let Some(my_timestamp) = self.pre_connection_timestamp {
                                                let payload = naia_shared::utils::read_headerless_payload(packet.payload());
                                                let mut reader = PacketReader::new(&payload);
                                                let payload_timestamp = Timestamp::read(&mut reader);

                                                if my_timestamp == payload_timestamp {
                                                    let mut digest_bytes: Vec<u8> = Vec::new();
                                                    for _ in 0..32 {
                                                        digest_bytes.push(reader.read_u8());
                                                    }
                                                    self.pre_connection_digest = Some(digest_bytes.into_boxed_slice());
                                                    self.connection_state = ClientConnectionState::AwaitingConnectResponse;
                                                }
                                            }
                                        }

                                        continue;
                                    }
                                    PacketType::ServerConnectResponse => {
                                        self.server_connection = Some(ServerConnection::new(self.server_address,
                                                                                            &self.config));
                                        self.connection_state = ClientConnectionState::Connected;
                                        output = Some(Ok(ClientEvent::Connection));
                                        continue;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        SocketEvent::None => {
                            output = Some(Ok(ClientEvent::None));
                            continue;
                        }
                    }
                }
                Err(error) => {
                    output = Some(Err(NaiaClientError::Wrapped(Box::new(error))));
                    continue;
                }
            }
        }
        return output.unwrap();
    }

    pub fn send_event(&mut self, event: &impl Event<T>) {

        if let Some(connection) = &mut self.server_connection {
            connection.queue_event(event);
        }
    }

    fn internal_send_with_connection(sender: &mut MessageSender,
                                     connection: &mut ServerConnection<T, U>,
                                     packet_type: PacketType,
                                     packet: Packet) {
        let new_payload = connection.process_outgoing_header(packet_type, packet.payload());
        sender.send(Packet::new_raw(new_payload))
            .expect("send failed!");
        connection.mark_sent();
    }

    fn internal_send_connectionless(sender: &mut MessageSender, packet_type: PacketType, packet: Packet) {
        let new_payload = naia_shared::utils::write_connectionless_payload(packet_type, packet.payload());
        sender.send(Packet::new_raw(new_payload))
            .expect("send failed!");
    }

    pub fn server_address(&self) -> SocketAddr {
        return self.socket.server_address();
    }

    pub fn get_sequence_number(&mut self) -> Option<u16> {
        if let Some(connection) = self.server_connection.as_mut() {
            return Some(connection.get_next_packet_index());
        }
        return None;
    }

    pub fn get_entity(&self, key: LocalEntityKey) -> Option<&U> {
        return self.server_connection.as_ref().unwrap().get_local_entity(key);
    }

    pub fn get_rtt(&self) -> f32 {
        return self.server_connection.as_ref().unwrap().get_rtt();
    }
}