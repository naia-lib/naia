use std::{
    net::{SocketAddr, UdpSocket},
    sync::{Arc, Mutex},
};

use crate::server_addr::ServerAddr;

/// Handles sending messages to the Server for a given Client Socket
#[derive(Clone)]
pub struct PacketSender {
    server_addr: SocketAddr,
    local_socket: Arc<Mutex<UdpSocket>>,
}

impl PacketSender {
    /// Create a new PacketSender, if supplied with the Server's address & a
    /// reference back to the parent Socket
    pub fn new(server_addr: SocketAddr, local_socket: Arc<Mutex<UdpSocket>>) -> Self {
        PacketSender {
            server_addr,
            local_socket,
        }
    }

    /// Send a Packet to the Server
    pub fn send(&self, payload: &[u8]) {
        //send it
        if self
            .local_socket
            .as_ref()
            .lock()
            .unwrap()
            .send_to(payload, self.server_addr)
            .is_err()
        {
            //TODO: handle this error
        }
    }

    /// Get the Server's Socket address
    pub fn server_addr(&self) -> ServerAddr {
        ServerAddr::Found(self.server_addr)
    }
}
