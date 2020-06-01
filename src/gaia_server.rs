
use std::{
    net::SocketAddr,
    collections::{VecDeque, HashMap},
};

use log::{info, error};

use gaia_server_socket::{ServerSocket, SocketEvent, MessageSender, Config as SocketConfig, GaiaServerSocketError};
pub use gaia_shared::{Config, PacketType, NetConnection, Timer, Timestamp, Manifest, NetEvent, ManagerType};

use super::server_event::ServerEvent;
use crate::{
    Packet,
    error::GaiaServerError};

const HOST_TYPE_NAME: &str = "Server";

pub struct GaiaServer {
    manifest: Manifest,
    config: Config,
    socket: ServerSocket,
    sender: MessageSender,
    client_connections: HashMap<SocketAddr, NetConnection>,
    outstanding_disconnects: VecDeque<SocketAddr>,
    heartbeat_timer: Timer,
    drop_counter: u8,
    drop_max: u8,
}

impl GaiaServer {
    pub async fn listen(address: &str, manifest: Manifest, config: Option<Config>) -> Self {

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
            manifest,
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
                        let payload = connection.process_outgoing(PacketType::Heartbeat, &[]);
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

                            match packet_type {
                                PacketType::ClientHandshake => {
                                    let payload = gaia_shared::utils::read_headerless_payload(packet.payload());
                                    let timestamp = Timestamp::read(&payload);

                                    if !self.client_connections.contains_key(&address) {
                                        self.client_connections.insert(address,
                                                                       NetConnection::new(self.config.heartbeat_interval,
                                                                                          self.config.disconnection_timeout_duration,
                                                                                          HOST_TYPE_NAME,
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
                                            let payload = connection.process_incoming(packet.payload());
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
                                            connection.process_incoming(packet.payload());
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

    pub async fn send_event(&mut self, addr: SocketAddr, event: impl NetEvent) {

//        Manifest::write_test(&mut payload);

//        //Write event payload
//        let mut event_payload_bytes = Vec::<u8>::new();
//        event.write(&mut event_payload_bytes);
//        if event_payload_bytes.len() > 255 {
//            error!("cannot encode an event with more than 255 bytes, need to implement this");
//        }
//
//        //Write event "header" (event id & payload length)
        let mut event_total_bytes = Vec::<u8>::new();
        Manifest::write_u8(1, &mut event_total_bytes);//DELETE
        Manifest::write_u16(4815, &mut event_total_bytes);//DELETE
//        self.manifest.write_gaia_id(event, &mut event_total_bytes); // write gaia id
//        event_total_bytes.push(event_payload_bytes.len() as u8); // write payload length
//        event_total_bytes.append(&mut event_payload_bytes); // write payload
//
//        //Write manager "header" (manager type & entity count)
//        let mut out_bytes = Vec::<u8>::new();
//        Manifest::write_manager_type(ManagerType::Event, &mut out_bytes); // write manager type
//        Manifest::write_u8(1, &mut out_bytes); // write number of events in the following message
//        out_bytes.append(&mut event_total_bytes); // write event payload

        self.send(Packet::new(addr, event_total_bytes))//should be out_bytes
                            .await;
    }

    pub async fn send(&mut self, packet: Packet) {
        self.send_internal(PacketType::Data, packet).await;
    }

    async fn send_internal(&mut self, packet_type: PacketType, packet: Packet) {
        if let Some(connection) = self.client_connections.get_mut(&packet.address()) {
            let payload = connection.process_outgoing(packet_type, packet.payload());
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
            return Some(connection.get_next_packet_index());
        }
        return None;
    }
}