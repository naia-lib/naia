
use std::{
    net::SocketAddr,
    error::Error,
};

use gaia_client_socket::{ClientSocket, SocketEvent, MessageSender, Config as SocketConfig};
pub use gaia_shared::{HeaderHandler, Config, PacketType, Timer};

use super::client_event::ClientEvent;
use crate::error::GaiaClientError;
use crate::Packet;

pub struct GaiaClient {
    socket: ClientSocket,
    sender: MessageSender,
    drop_counter: u8,
    header_handler: HeaderHandler,
    config: Config,
    connected: bool,
    handshake_timer: Timer,
    heartbeat_timer: Timer,
    timeout_timer: Timer,
}

impl GaiaClient {
    pub fn connect(server_address: &str, config: Option<Config>) -> Self {

        let config = match config {
            Some(config) => config,
            None => Config::default()
        };

        let mut socket_config = SocketConfig::default();
        socket_config.connectionless = true;
        let mut client_socket = ClientSocket::connect(&server_address, Some(socket_config));

        let handshake_timer = Timer::new(config.send_handshake_interval);
        let heartbeat_timer = Timer::new(config.heartbeat_interval);
        let timeout_timer = Timer::new(config.disconnection_timeout_duration);
        let message_sender = client_socket.get_sender();

        GaiaClient {
            socket: client_socket,
            sender: message_sender,
            drop_counter: 0,
            header_handler: HeaderHandler::new(),
            config,
            connected: false,
            handshake_timer,
            heartbeat_timer,
            timeout_timer,
        }
    }

    pub fn receive(&mut self) -> Result<ClientEvent, GaiaClientError> {

        // send handshakes, send heartbeats, timeout if need be
        if self.connected {
            if self.timeout_timer.ringing() {
                self.connected = false;
                return Ok(ClientEvent::Disconnection);
            }
            if self.heartbeat_timer.ringing() {
                let outpacket = self.header_handler.process_outgoing(PacketType::Heartbeat, &[]);
                self.sender.send(Packet::new_raw(outpacket));
                self.heartbeat_timer.reset();
            }
        }
        else {
            if self.handshake_timer.ringing() {
                let outpacket = self.header_handler.process_outgoing(PacketType::ClientHandshake, &[]);
                self.sender.send(Packet::new_raw(outpacket));
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
                                ///////////Simulating dropping/////////
                            if self.drop_counter > 2 {
                                self.drop_counter = 0;
                                output = Some(Ok(ClientEvent::None));
                            } else {
                                self.drop_counter += 1;

                                ///////////this logic stays//////////
                                self.timeout_timer.reset();
                                let (packet_type, new_payload) = self.header_handler.process_incoming(packet.payload());
                                match packet_type {
                                    PacketType::ServerHandshake => {
                                        if !self.connected {
                                            self.connected = true;
                                            output = Some(Ok(ClientEvent::Connection));
                                        }
                                    }
                                    PacketType::Data => {
                                        let newstr = String::from_utf8_lossy(&new_payload).to_string();
                                        output = Some(Ok(ClientEvent::Message(newstr)));
                                    }
                                    _ => {}
                                }
                                //////////////////////////////////////
                            }
                        }
                        SocketEvent::None => {
                            output = Some(Ok(ClientEvent::None));
                        }
                        _ => {
                            // We are not using Socket Connection/Disconnection Events
                        }
                    }
                }
                Err(error) => {
                    output = Some(Err(GaiaClientError::Wrapped(Box::new(error))));
                }
            }
        }
        return output.unwrap();
    }

    pub fn send(&mut self, packet: Packet) {
        let new_payload = self.header_handler.process_outgoing(PacketType::Data, packet.payload());
        self.sender.send(Packet::new_raw(new_payload));
        self.heartbeat_timer.reset();
    }

    pub fn server_address(&self) -> SocketAddr {
        return self.socket.server_address();
    }
}