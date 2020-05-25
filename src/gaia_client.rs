
use std::{
    net::SocketAddr,
    error::Error,
};

use gaia_client_socket::{ClientSocket, SocketEvent, MessageSender, Config};

use super::client_event::ClientEvent;
use crate::error::GaiaClientError;

pub struct GaiaClient {
    socket: ClientSocket,
    sender: MessageSender,
}

impl GaiaClient {
    pub fn connect(server_address: &str) -> Self {
        let mut client_socket = ClientSocket::connect(&server_address, Some(Config::default()));

        let message_sender = client_socket.get_sender();

        GaiaClient {
            socket: client_socket,
            sender: message_sender,
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
                    SocketEvent::Message(message) => {
                        Ok(ClientEvent::Message(message))
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

    pub fn send(&mut self, message: String) -> Result<(), Box<dyn Error + Send>> {
        self.sender.send(message)
    }

    pub fn server_address(&self) -> SocketAddr {
        return self.socket.server_address();
    }
}