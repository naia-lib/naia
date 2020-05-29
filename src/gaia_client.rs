
use std::{
    net::SocketAddr,
    error::Error,
};

use log::info;

use gaia_client_socket::{ClientSocket, SocketEvent, MessageSender, Config as SocketConfig};
pub use gaia_shared::{Config, PacketType, Timer, NetConnection};

use super::client_event::ClientEvent;
use crate::error::GaiaClientError;
use crate::Packet;

pub struct GaiaClient {
    socket: ClientSocket,
    sender: MessageSender,
    drop_counter: u8,
    drop_max: u8,
    config: Config,
    connected: bool,
    handshake_timer: Timer,
    server_connection: NetConnection,
}

impl GaiaClient {
    pub fn connect(server_address: &str, config: Option<Config>) -> Self {

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
        let server_connection = NetConnection::new(config.heartbeat_interval, config.disconnection_timeout_duration, "Client");
        let message_sender = client_socket.get_sender();

        GaiaClient {
            socket: client_socket,
            sender: message_sender,
            drop_counter: 1,
            drop_max: 3,
            config,
            connected: false,
            handshake_timer,
            server_connection,
        }
    }

    pub fn receive(&mut self) -> Result<ClientEvent, GaiaClientError> {

        // send handshakes, send heartbeats, timeout if need be
        if self.connected {
            if self.server_connection.should_drop() {
                self.connected = false;
                return Ok(ClientEvent::Disconnection);
            }
            if self.server_connection.should_send_heartbeat() {
                self.send_internal(PacketType::Heartbeat, Packet::empty());
                self.server_connection.mark_sent();
            }
        }
        else {
            if self.handshake_timer.ringing() {
                self.send_internal(PacketType::ClientHandshake, Packet::empty());
                self.handshake_timer.reset();
            }
        }

        // receive from socket
        let mut output: Option<Result<ClientEvent, GaiaClientError>> = None;
        while output.is_none() {
            match self.socket.receive() {
                Ok(event) => {
                    match event {
                        SocketEvent::Packet(packet) => {
                            self.server_connection.mark_heard();

                            if PacketType::get_from_packet(packet.payload()) == PacketType::Data {
                                //simulate dropping
                                if self.drop_counter >= self.drop_max {
                                    self.drop_counter = 0;
                                    info!("~~~~~~~~~~  dropped packet from server  ~~~~~~~~~~");
                                    continue;
                                } else {
                                    self.drop_counter += 1;
                                }
                            }
                            let packet_type = PacketType::get_from_packet(packet.payload());
                            let payload = self.server_connection.ack_manager.process_incoming(packet.payload());

                            match packet_type {
                                PacketType::ServerHandshake => {
                                    if !self.connected {
                                        self.connected = true;
                                        output = Some(Ok(ClientEvent::Connection));
                                        continue;
                                    }
                                }
                                PacketType::Data => {
                                    let newstr = String::from_utf8_lossy(&payload).to_string();
                                    output = Some(Ok(ClientEvent::Message(newstr)));
                                    continue;
                                }
                                PacketType::Heartbeat => {
                                    info!("<- s");
                                }
                                _ => {}
                            }
                        }
                        SocketEvent::None => {
                            output = Some(Ok(ClientEvent::None));
                            continue;
                        }
                        _ => {
                            // We are not using Socket Connection/Disconnection Events
                        }
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

    pub fn send(&mut self, packet: Packet) {
        self.send_internal(PacketType::Data, packet)
    }

    fn send_internal(&mut self, packet_type: PacketType, packet: Packet) {
        let new_payload = self.server_connection.ack_manager.process_outgoing(packet_type, packet.payload());
        self.sender.send(Packet::new_raw(new_payload))
            .expect("send failed!");
        self.server_connection.mark_sent();
    }

    pub fn server_address(&self) -> SocketAddr {
        return self.socket.server_address();
    }

    pub fn get_sequence_number(&mut self) -> u16 {
        return self.server_connection.ack_manager.local_sequence_num();
    }
}