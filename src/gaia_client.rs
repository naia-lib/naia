
use std::{
    net::SocketAddr,
    error::Error,
};

use gaia_client_socket::{ClientSocket, SocketEvent, MessageSender, Config};
pub use gaia_shared::AckHandler;

use super::client_event::ClientEvent;
use crate::error::GaiaClientError;
use crate::Packet;

pub struct GaiaClient {
    socket: ClientSocket,
    sender: MessageSender,
    drop_counter: u8,
    ack_handler: AckHandler,
}

impl GaiaClient {
    pub fn connect(server_address: &str) -> Self {
        let mut client_socket = ClientSocket::connect(&server_address, Some(Config::default()));

        let message_sender = client_socket.get_sender();

        GaiaClient {
            socket: client_socket,
            sender: message_sender,
            drop_counter: 0,
            ack_handler: AckHandler::new(),
        }
    }

    pub fn receive(&mut self) -> Result<ClientEvent, GaiaClientError> {
        match self.socket.receive() {
            Ok(event) => {
                match event {
                    SocketEvent::Connection => {
                        Ok(ClientEvent::Connection)
                    }
                    SocketEvent::Disconnection => {
                        Ok(ClientEvent::Disconnection)
                    }
                    SocketEvent::Packet(packet) => {
                        //Simulating dropping
                        if self.drop_counter > 2 {
                            self.drop_counter = 0;
                            return Ok(ClientEvent::None)
                        } else {
                            //self.drop_counter += 1;
                            //this logic stays//
                            let new_payload = self.ack_handler.process_incoming(packet.payload());
                            let newstr = String::from_utf8_lossy(&new_payload).to_string();
                            Ok(ClientEvent::Message(newstr))
                            ////////////////////
                        }

                    }
                    SocketEvent::None => {
                        Ok(ClientEvent::None)
                    }
                }
            }
            Err(error) => {
                Err(GaiaClientError::Wrapped(Box::new(error)))
            }
        }
    }

    pub fn send(&mut self, packet: Packet) {
        let new_payload = self.ack_handler.process_outgoing(packet.payload());
        self.sender.send(Packet::new_raw(new_payload));
    }

    pub fn server_address(&self) -> SocketAddr {
        return self.socket.server_address();
    }
}