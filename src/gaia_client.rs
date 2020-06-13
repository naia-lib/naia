
use std::{
    net::SocketAddr,
};

use log::info;

use gaia_client_socket::{ClientSocket, SocketEvent, MessageSender, Config as SocketConfig};
pub use gaia_shared::{Config, LocalEntityKey, PacketType, Timer, Timestamp,
                      Manifest, ManagerType, HostType, PacketWriter, PacketReader,
                      NetEvent, EventType, EntityType};

use super::{
    server_connection::ServerConnection,
    client_event::ClientEvent,
    client_entity_message::ClientEntityMessage,
    error::GaiaClientError,
    Packet
};

pub struct GaiaClient<T: EventType, U: EntityType> {
    manifest: Manifest<T, U>,
    server_address: SocketAddr,
    config: Config,
    socket: ClientSocket,
    sender: MessageSender,
    server_connection: Option<ServerConnection<T, U>>,
    pre_connection_timestamp: Option<Timestamp>,
    handshake_timer: Timer,
    drop_counter: u8,
    drop_max: u8,
}

impl<T: EventType, U: EntityType> GaiaClient<T, U> {
    pub fn connect(server_address: &str, manifest: Manifest<T, U>, config: Option<Config>) -> Self {

        let mut config = match config {
            Some(config) => config,
            None => Config::default()
        };
        config.heartbeat_interval /= 2;

        let mut socket_config = SocketConfig::default();
        socket_config.connectionless = true;
        let mut client_socket = ClientSocket::connect(&server_address, Some(socket_config));

        let mut handshake_timer = Timer::new(config.send_handshake_interval);
        handshake_timer.ring_manual();
        let message_sender = client_socket.get_sender();

        GaiaClient {
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
        }
    }

    pub fn receive(&mut self) -> Result<ClientEvent<T, U>, GaiaClientError> {

        // send handshakes, send heartbeats, timeout if need be
        match &mut self.server_connection {
            Some(connection) => {
                if connection.should_drop() {
                    self.server_connection = None;
                    return Ok(ClientEvent::Disconnection);
                }
                if connection.should_send_heartbeat() {
                    GaiaClient::internal_send_with_connection(&mut self.sender, connection, PacketType::Heartbeat, Packet::empty());
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
                        ClientEntityMessage::Create(local_key, entity) => {
                            return Ok(ClientEvent::CreateEntity(local_key, entity));
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

                    if self.pre_connection_timestamp.is_none() {
                        self.pre_connection_timestamp = Some(Timestamp::now());
                    }

                    let mut timestamp_bytes = Vec::new();
                    self.pre_connection_timestamp.as_mut().unwrap().write(&mut timestamp_bytes);
                    GaiaClient::<T,U>::internal_send_connectionless(&mut self.sender, PacketType::ClientHandshake, Packet::new(timestamp_bytes));
                    self.handshake_timer.reset();
                }
            }
        }

        // receive from socket
        let mut output: Option<Result<ClientEvent<T, U>, GaiaClientError>> = None;
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
                                if packet_type == PacketType::ServerHandshake {
                                    self.server_connection = Some(ServerConnection::new(self.server_address,
                                                                                        self.config.heartbeat_interval,
                                                                                        self.config.disconnection_timeout_duration,
                                                                                        self.pre_connection_timestamp.take().unwrap()));
                                    output = Some(Ok(ClientEvent::Connection));
                                    continue;
                                }
                            }
                        }
                        SocketEvent::None => {
                            output = Some(Ok(ClientEvent::None));
                            continue;
                        }
                        _ => {} // We are not using Socket Connection/Disconnection Events
                    }
                }
                Err(error) => {
                    output = Some(Err(GaiaClientError::Wrapped(Box::new(error))));
                    continue;
                }
            }
        }
        return output.unwrap();
    }

    pub fn send_event(&mut self, event: &impl NetEvent<T>) {

        if let Some(connection) = &mut self.server_connection {
            connection.queue_event(event);
        }
    }

    fn internal_send_with_connection(sender: &mut MessageSender, connection: &mut ServerConnection<T, U>, packet_type: PacketType, packet: Packet) {
        let new_payload = connection.process_outgoing_header(packet_type, packet.payload());
        sender.send(Packet::new_raw(new_payload))
            .expect("send failed!");
        connection.mark_sent();
    }

    fn internal_send_connectionless(sender: &mut MessageSender, packet_type: PacketType, packet: Packet) {
        let new_payload = gaia_shared::utils::write_connectionless_payload(packet_type, packet.payload());
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
}