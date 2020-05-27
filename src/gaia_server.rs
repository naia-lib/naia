
use std::{
    net::SocketAddr,
    error::Error,
    collections::{VecDeque, HashMap},
};

use log::info;

use gaia_server_socket::{ServerSocket, SocketEvent, MessageSender, Config as SocketConfig};
pub use gaia_shared::{HeaderHandler, Config, PacketType, ConnectionManager, Timer};

use super::server_event::ServerEvent;
use crate::error::GaiaServerError;
use crate::Packet;

pub struct GaiaServer {
    socket: ServerSocket,
    sender: MessageSender,
    drop_counter: u8,
    header_handler: HeaderHandler,
    config: Config,
    clients: HashMap<SocketAddr, ConnectionManager>,
    outstanding_disconnects: VecDeque<SocketAddr>,
    heartbeat_timer: Timer,
}

impl GaiaServer {
    pub async fn listen(address: &str, config: Option<Config>) -> Self {

        let mut config = match config {
            Some(config) => config,
            None => Config::default()
        };
        config.heartbeat_interval /= 4;

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
            drop_counter: 0,
            header_handler: HeaderHandler::new(),
            config,
            clients: clients_map,
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

                for (address, connection) in self.clients.iter_mut() {
                    if connection.should_drop() {
                        self.outstanding_disconnects.push_back(*address);
                    } else if connection.should_send_heartbeat() {
                        let outpacket = self.header_handler.process_outgoing(PacketType::Heartbeat, &[]);
                        self.sender.send(Packet::new_raw(*address, outpacket)).await;
                        connection.mark_sent();
                    }
                }
            }

            // timeouts
            if let Some(addr) = self.outstanding_disconnects.pop_front() {
                self.clients.remove(&addr);
                output = Some(Ok(ServerEvent::Disconnection(addr)));
                continue;
            }

            //receive socket events
            match self.socket.receive().await {
                Ok(event) => {
                    match event {
                        SocketEvent::Packet(packet) => {
                            //Simulating dropping///////////////
                            if self.drop_counter > 5 {
                                self.drop_counter = 0;
                            } else {
                                self.drop_counter += 1;

                                //this logic stays////////////////////

                                let address = packet.address();
                                match self.clients.get_mut(&address) {
                                    Some(connection) => {
                                        connection.mark_heard();
                                    }
                                    None => {} //not yet established connection
                                }

                                let (packet_type, new_payload) = self.header_handler.process_incoming(packet.payload());
                                match packet_type {
                                    PacketType::ClientHandshake => {
                                        // Send Server
                                        let to_client_message = "trying to shake yo hand yo".to_string().into_bytes().into_boxed_slice();
                                        let outpacket = self.header_handler.process_outgoing(PacketType::ServerHandshake, &to_client_message);
                                        self.sender.send(Packet::new_raw(address, outpacket)).await;

                                        if !self.clients.contains_key(&address) {
                                            self.clients.insert(address, ConnectionManager::new(self.config.heartbeat_interval, self.config.disconnection_timeout_duration));
                                            output = Some(Ok(ServerEvent::Connection(address)));
                                            continue;
                                        }
                                    }
                                    PacketType::Data => {
                                        if self.clients.contains_key(&address) {
                                            let newstr = String::from_utf8_lossy(&new_payload).to_string();
                                            output = Some(Ok(ServerEvent::Message(packet.address(), newstr)));
                                            continue;
                                        } else {
                                            warn!("received data from unauthenticated client: {}", address);
                                        }
                                    }
                                    PacketType::Heartbeat => {
                                        info!("Heartbeat from Client");
                                    }
                                    _ => {}
                                }
                                //////////////////////////////////////
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
                    output = Some(Err(GaiaServerError::Wrapped(Box::new(error))));
                    continue;
                }
            }
        }
        return output.unwrap();
    }

    pub async fn send(&mut self, packet: Packet) {
        let new_payload = self.header_handler.process_outgoing(PacketType::Data, packet.payload());
        self.sender.send(Packet::new_raw(packet.address(), new_payload)).await;
        if let Some(connection) = self.clients.get_mut(&packet.address()) {
            connection.mark_sent();
        }
    }

    pub fn get_clients(&mut self) -> Vec<SocketAddr> {
        self.clients.keys().cloned().collect()
    }
}