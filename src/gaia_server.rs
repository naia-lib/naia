
use std::{
    net::SocketAddr,
    collections::{VecDeque, HashMap},
};

use log::info;

use gaia_server_socket::{ServerSocket, SocketEvent, MessageSender, Config as SocketConfig, GaiaServerSocketError};
pub use gaia_shared::{Config, PacketType, NetConnection, Timer, Timestamp};

use super::server_event::ServerEvent;
use crate::error::GaiaServerError;
use crate::Packet;

pub struct GaiaServer {
    socket: ServerSocket,
    sender: MessageSender,
    drop_counter: u8,
    drop_max: u8,
    config: Config,
    client_connections: HashMap<SocketAddr, NetConnection>,
    outstanding_disconnects: VecDeque<SocketAddr>,
    heartbeat_timer: Timer,
}

impl GaiaServer {
    pub async fn listen(address: &str, config: Option<Config>) -> Self {

        let mut config = match config {
            Some(config) => config,
            None => Config::default()
        };
        config.heartbeat_interval /= 2;

        let mut socket_config = SocketConfig::default();
        socket_config.connectionless = true;
        socket_config.tick_interval = config.tick_interval;
        let mut server_socket = ServerSocket::listen(address, Some(socket_config)).await;

        let sender = server_socket.get_sender();
        let clients_map = HashMap::new();
        let heartbeat_timer = Timer::new(config.heartbeat_interval);

        GaiaServer {
            socket: server_socket,
            sender,
            drop_counter: 1,
            drop_max: 3,
            config,
            client_connections: clients_map,
            outstanding_disconnects: VecDeque::new(),
            heartbeat_timer,
        }
    }

    pub async fn receive(&mut self) -> Result<ServerEvent, GaiaServerError> {
        let mut output: Option<Result<ServerEvent, GaiaServerError>> = None;
        while output.is_none() {

            // heartbeats
            if self.heartbeat_timer.ringing() {
                self.heartbeat_timer.reset();

                for (address, connection) in self.client_connections.iter_mut() {
                    if connection.should_drop() {
                        self.outstanding_disconnects.push_back(*address);
                    } else if connection.should_send_heartbeat() {
                        // Don't try to refactor this to self.internal_send, doesn't seem to work
                        let payload = connection.ack_manager.process_outgoing(PacketType::Heartbeat, &[]);
                        self.sender.send(Packet::new_raw(*address, payload))
                            .await
                            .expect("send failed!");
                        connection.mark_sent();
                    }
                }
            }

            // timeouts
            if let Some(addr) = self.outstanding_disconnects.pop_front() {
                self.client_connections.remove(&addr);
                output = Some(Ok(ServerEvent::Disconnection(addr)));
                continue;
            }

            //receive socket events
            match self.socket.receive().await {
                Ok(event) => {
                    match event {
                        SocketEvent::Packet(packet) => {
                            let address = packet.address();
                            match self.client_connections.get_mut(&address) {
                                Some(connection) => {
                                    connection.mark_heard();
                                }
                                None => {} //not yet established connection
                            }

                            let packet_type = PacketType::get_from_packet(packet.payload());
                            if packet_type == PacketType::Data {
                                //simulate dropping
                                if self.drop_counter >= self.drop_max {
                                    self.drop_counter = 0;
                                    info!("~~~~~~~~~~  dropped packet from client  ~~~~~~~~~~");
                                    continue;
                                } else {
                                    self.drop_counter += 1;
                                }
                            }

                            //let connection = self.client_connections.get(&address);
                            //let new_payload = connection.ack_manager.process_incoming(packet.payload());

                            match packet_type {
                                PacketType::ClientHandshake => {
                                    let payload = NetConnection::get_headerless_payload(packet.payload());
                                    let timestamp = Timestamp::read(&payload);

                                    if !self.client_connections.contains_key(&address) {
                                        self.client_connections.insert(address,
                                                                       NetConnection::new(self.config.heartbeat_interval,
                                                                                          self.config.disconnection_timeout_duration,
                                                                                          "Server",
                                                                                          timestamp));
                                        output = Some(Ok(ServerEvent::Connection(address)));
                                    }

                                    match self.client_connections.get_mut(&address) {
                                        Some(connection) => {
                                            if timestamp == connection.connection_timestamp {
                                                self.send_internal(PacketType::ServerHandshake, Packet::new_raw(address, Box::new([])))
                                                    .await;
                                                continue;
                                            } else {
                                                // Incoming Timestamp is different than recorded.. must be the same client trying to connect..
                                                // so disconnect them to provide continuity
                                                self.client_connections.remove(&address);
                                                output = Some(Ok(ServerEvent::Disconnection(address)));
                                                continue;
                                            }
                                        }
                                        None => {}
                                    }
                                }
                                PacketType::Data => {

                                    match self.client_connections.get_mut(&address) {
                                        Some(connection) => {
                                            let payload = connection.ack_manager.process_incoming(packet.payload());
                                            let newstr = String::from_utf8_lossy(&payload).to_string();
                                            output = Some(Ok(ServerEvent::Message(packet.address(), newstr)));
                                            continue;
                                        }
                                        None => {
                                            warn!("received data from unauthenticated client: {}", address);
                                        }
                                    }
                                }
                                PacketType::Heartbeat => {
                                    match self.client_connections.get_mut(&address) {
                                        Some(connection) => {
                                            // Still need to do this so that proper notify events fire based on the heartbeat header
                                            connection.ack_manager.process_incoming(packet.payload());
                                            info!("<- c");
                                            continue;
                                        }
                                        None => {
                                            warn!("received heartbeat from unauthenticated client: {}", address);
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        SocketEvent::Tick => {
                            output = Some(Ok(ServerEvent::Tick));
                            continue;
                        }
                        _ => {} // We are not using Socket Connection/Disconnection Events
                    }
                }
                Err(error) => {
                    if let GaiaServerSocketError::SendError(address) = error {
                        self.client_connections.remove(&address);
                        output = Some(Ok(ServerEvent::Disconnection(address)));
                        continue;
                    }

                    output = Some(Err(GaiaServerError::Wrapped(Box::new(error))));
                    continue;
                }
            }
        }
        return output.unwrap();
    }

    pub async fn send(&mut self, packet: Packet) {
        self.send_internal(PacketType::Data, packet).await;
    }

    async fn send_internal(&mut self, packet_type: PacketType, packet: Packet) {
        if let Some(connection) = self.client_connections.get_mut(&packet.address()) {
            let payload = connection.ack_manager.process_outgoing(packet_type, packet.payload());
            match self.sender.send(Packet::new_raw(packet.address(), payload))
                .await {
                Ok(_) => {}
                Err(err) => {
                    info!("send error! {}", err);
                }
            }
            connection.mark_sent();
        }
    }

    pub fn get_clients(&mut self) -> Vec<SocketAddr> {
        self.client_connections.keys().cloned().collect()
    }

    pub fn get_sequence_number(&mut self, addr: SocketAddr) -> Option<u16> {
        if let Some(connection) = self.client_connections.get_mut(&addr) {
            return Some(connection.ack_manager.local_sequence_num());
        }
        return None;
    }
}