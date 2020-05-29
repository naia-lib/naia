
use std::{
    net::SocketAddr,
};

use log::info;

use gaia_client_socket::{ClientSocket, SocketEvent, MessageSender, Config as SocketConfig};
pub use gaia_shared::{Config, PacketType, Timer, NetConnection, Timestamp};

use super::client_event::ClientEvent;
use crate::{
    Packet, error::GaiaClientError};

const HOST_TYPE_NAME: &str = "CLIENT";

pub struct GaiaClient {
    socket: ClientSocket,
    sender: MessageSender,
    drop_counter: u8,
    drop_max: u8,
    config: Config,
    handshake_timer: Timer,
    server_connection: Option<NetConnection>,
    pre_connection_timestamp: Option<Timestamp>,
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
        let message_sender = client_socket.get_sender();

        GaiaClient {
            socket: client_socket,
            sender: message_sender,
            drop_counter: 1,
            drop_max: 3,
            config,
            handshake_timer,
            server_connection: None,
            pre_connection_timestamp: None,
        }
    }

    pub fn receive(&mut self) -> Result<ClientEvent, GaiaClientError> {

        // send handshakes, send heartbeats, timeout if need be
        if self.server_connection.is_some() {
            if self.server_connection.as_ref().unwrap().should_drop() {
                self.server_connection = None;
                return Ok(ClientEvent::Disconnection);
            }
            if self.server_connection.as_ref().unwrap().should_send_heartbeat() {
                self.send_internal(PacketType::Heartbeat, Packet::empty());
                self.server_connection.as_mut().unwrap().mark_sent();
            }
        }
        else {
            if self.handshake_timer.ringing() {

                if self.pre_connection_timestamp.is_none() {
                    self.pre_connection_timestamp = Some(Timestamp::now());
                }

                let mut timestamp_bytes = Vec::new();
                self.pre_connection_timestamp.as_mut().unwrap().write(&mut timestamp_bytes);
                self.send_internal(PacketType::ClientHandshake, Packet::new(timestamp_bytes));
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

                            if let Some(server_connection) = self.server_connection.as_mut() {
                                server_connection.mark_heard();
                                let payload = server_connection.process_incoming(packet.payload());

                                match packet_type {
                                    PacketType::Data => {
                                        let newstr = String::from_utf8_lossy(&payload).to_string();
                                        output = Some(Ok(ClientEvent::Message(newstr)));
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
                                    self.server_connection = Some(NetConnection::new(self.config.heartbeat_interval,
                                                                                     self.config.disconnection_timeout_duration,
                                                                                     HOST_TYPE_NAME,
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

    pub fn send(&mut self, packet: Packet) {
        self.send_internal(PacketType::Data, packet)
    }

    fn send_internal(&mut self, packet_type: PacketType, packet: Packet) {
        if let Some(server_connection) = self.server_connection.as_mut() {
            let new_payload = server_connection.process_outgoing(packet_type, packet.payload());
            self.sender.send(Packet::new_raw(new_payload))
                .expect("send failed!");
            server_connection.mark_sent();
        }
        else {
            let new_payload = gaia_shared::utils::write_connectionless_payload(packet_type, packet.payload());
            self.sender.send(Packet::new_raw(new_payload))
                .expect("send failed!");
        }
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
}