
use std::{
    net::SocketAddr,
    error::Error,
};

use log::info;

use gaia_server_socket::{ServerSocket, SocketEvent, MessageSender, Config as SocketConfig};
pub use gaia_shared::{AckHandler, Config};

use super::server_event::ServerEvent;
use crate::error::GaiaServerError;
use crate::Packet;

pub struct GaiaServer {
    socket: ServerSocket,
    sender: MessageSender,
    drop_counter: u8,
    ack_handler: AckHandler,
    config: Config,
}

impl GaiaServer {
    pub async fn listen(address: &str, config: Option<Config>) -> Self {

        let config = match config {
            Some(config) => config,
            None => Config::default()
        };

        let mut socket_config = SocketConfig::default();
        socket_config.connectionless = true;
        socket_config.tick_interval = config.tick_interval;
        let mut server_socket = ServerSocket::listen(address, Some(socket_config)).await;

        let sender = server_socket.get_sender();

        GaiaServer {
            socket: server_socket,
            sender,
            drop_counter: 0,
            ack_handler: AckHandler::new(),
            config,
        }
    }

    pub async fn receive(&mut self) -> Result<ServerEvent, GaiaServerError> {
        let mut output: Option<Result<ServerEvent, GaiaServerError>> = None;
        while output.is_none() {
            match self.socket.receive().await {
                Ok(event) => {
                    match event {
                        SocketEvent::Packet(packet) => {
                            //Simulating dropping
                            if self.drop_counter > 5 {
                                self.drop_counter = 0;
                            } else {
                                self.drop_counter += 1;
                                //this logic stays//
                                let new_payload = self.ack_handler.process_incoming(packet.payload());
                                let newstr = String::from_utf8_lossy(&new_payload).to_string();
                                output = Some(Ok(ServerEvent::Message(packet.address(), newstr)));
                                ////////////////////
                            }
                        }
                        SocketEvent::Tick => {
                            output = Some(Ok(ServerEvent::Tick));
                        }
                        _ => {
                            // We are not using Socket Connection/Disconnection Events
                        }
                    }
                }
                Err(error) => {
                    output = Some(Err(GaiaServerError::Wrapped(Box::new(error))));
                }
            }
        }
        return output.unwrap();
    }

    pub async fn send(&mut self, packet: Packet) {
        let new_payload = self.ack_handler.process_outgoing(packet.payload());
        self.sender.send(Packet::new_raw(packet.address(), new_payload)).await;
    }

    pub fn get_clients(&mut self) -> Vec<SocketAddr> {
        self.socket.get_clients()
    }
}