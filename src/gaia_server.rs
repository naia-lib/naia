
use std::{
    net::SocketAddr,
    error::Error,
};

use log::info;

use gaia_server_socket::{ServerSocket, SocketEvent, MessageSender, Config};
pub use gaia_shared::AckHandler;

use super::server_event::ServerEvent;
use crate::error::GaiaServerError;

pub struct GaiaServer {
    socket: ServerSocket,
    sender: MessageSender,
    drop_counter: u8,
    ack_handler: AckHandler,
}

impl GaiaServer {
    pub async fn listen(address: &str) -> Self {

        let mut server_socket = ServerSocket::listen(address, Some(Config::default())).await;

        let sender = server_socket.get_sender();

        GaiaServer {
            socket: server_socket,
            sender,
            drop_counter: 0,
            ack_handler: AckHandler::new(),
        }
    }

    pub async fn receive(&mut self) -> Result<ServerEvent, GaiaServerError> {
        let mut output: Option<Result<ServerEvent, GaiaServerError>> = None;
        while output.is_none() {
            match self.socket.receive().await {
                Ok(event) => {
                    match event {
                        SocketEvent::Connection(address) => {
                            output = Some(Ok(ServerEvent::Connection(address)));
                        }
                        SocketEvent::Disconnection(address) => {
                            output = Some(Ok(ServerEvent::Disconnection(address)));
                        }
                        SocketEvent::Message(address, message) => {
                            //Simulating dropping
                            if self.drop_counter > 3 {
                                self.drop_counter = 0;
                            } else {
                                //self.drop_counter += 1;
                                //this logic stays//
                                let message = self.ack_handler.process_incoming(message);
                                output = Some(Ok(ServerEvent::Message(address, message)));
                                ////////////////////
                            }
                        }
                        SocketEvent::Tick => {
                            output = Some(Ok(ServerEvent::Tick));
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

    pub async fn send(&mut self, message: (SocketAddr, String)) {
        let (address, message) = message;
        let message = self.ack_handler.process_outgoing(message);
        self.sender.send((address, message)).await;
    }

    pub fn get_clients(&mut self) -> Vec<SocketAddr> {
        self.socket.get_clients()
    }
}