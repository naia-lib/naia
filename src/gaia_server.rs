
use std::{
    net::SocketAddr,
    error::Error,
};

use gaia_server_socket::{ServerSocket, SocketEvent, MessageSender, Config};

use super::server_event::ServerEvent;
use crate::error::GaiaServerError;

pub struct GaiaServer {
    socket: ServerSocket,
    sender: MessageSender,
}

impl GaiaServer {
    pub async fn listen(address: &str) -> Self {

        let mut server_socket = ServerSocket::listen(address, Some(Config::default())).await;

        let sender = server_socket.get_sender();

        GaiaServer {
            socket: server_socket,
            sender,
        }
    }

    pub async fn receive(&mut self) -> Result<ServerEvent, GaiaServerError> {
        match self.socket.receive().await {
            Ok(event) => {
                match event {
                    SocketEvent::Connection(address) => {
                        Ok(ServerEvent::Connection(address))
                    }
                    SocketEvent::Disconnection(address) => {
                        Ok(ServerEvent::Disconnection(address))
                    }
                    SocketEvent::Message(address, message) => {
                        Ok(ServerEvent::Message(address, message))
                    }
                    SocketEvent::Tick => {
                        Ok(ServerEvent::Tick)
                    }
                }
            }
            Err(error) => {
                Err(GaiaServerError::Wrapped(Box::new(error)))
            }
        }
    }

    pub async fn send(&mut self, message: (SocketAddr, String)) -> Result<(), Box<dyn Error + Send>> {
        return self.sender.send(message).await;
    }
}