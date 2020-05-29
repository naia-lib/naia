
use std::{
    net::SocketAddr,
    error::Error,
};

use log::info;

use gaia_client_socket::{ClientSocket, SocketEvent, MessageSender, Config as SocketConfig};
pub use gaia_shared::{HeaderHandler, Config, PacketType, Timer, ConnectionManager};

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
    connection_manager: ConnectionManager,
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
        let connection_manager = ConnectionManager::new(config.heartbeat_interval, config.disconnection_timeout_duration);
        let message_sender = client_socket.get_sender();

        GaiaClient {
            socket: client_socket,
            sender: message_sender,
            drop_counter: 0,
            header_handler: HeaderHandler::new(),
            config,
            connected: false,
            handshake_timer,
            connection_manager
        }
    }

    pub fn receive(&mut self) -> Result<ClientEvent, GaiaClientError> {

        // send handshakes, send heartbeats, timeout if need be
        if self.connected {
            if self.connection_manager.should_drop() {
                self.connected = false;
                return Ok(ClientEvent::Disconnection);
            }
            if self.connection_manager.should_send_heartbeat() {
                let outpacket = self.header_handler.process_outgoing(PacketType::Heartbeat, &[]);
                self.sender.send(Packet::new_raw(outpacket));
                self.connection_manager.mark_sent();
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
                            self.connection_manager.mark_heard();

                            if HeaderHandler::get_packet_type(packet.payload()) == PacketType::Data {
                                //simulate dropping
                                if self.drop_counter > 3 {
                                    self.drop_counter = 0;
                                    continue;
                                } else {
                                    self.drop_counter += 1;
                                }
                            }
                            let (packet_type, new_payload) = self.header_handler.process_incoming(packet.payload());

                            match packet_type {
                                PacketType::ServerHandshake => {
                                    if !self.connected {
                                        self.connected = true;
                                        output = Some(Ok(ClientEvent::Connection));
                                        continue;
                                    }
                                }
                                PacketType::Data => {
                                    //if self.connected {
                                        let newstr = String::from_utf8_lossy(&new_payload).to_string();
                                        output = Some(Ok(ClientEvent::Message(newstr)));
                                        continue;
                                    //}
                                }
                                PacketType::Heartbeat => {
                                    info!("Server Heartbeat");
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
        let new_payload = self.header_handler.process_outgoing(PacketType::Data, packet.payload());
        self.sender.send(Packet::new_raw(new_payload));
        self.connection_manager.mark_sent();
    }

    pub fn server_address(&self) -> SocketAddr {
        return self.socket.server_address();
    }

    pub fn get_sequence_number(&mut self) -> u16 {
        return self.header_handler.local_sequence_num();
    }
}