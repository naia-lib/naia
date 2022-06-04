use js_sys::Uint8Array;
use web_sys::MessagePort;

use crate::server_addr::ServerAddr;

use super::{addr_cell::AddrCell, data_port::DataPort};

/// Handles sending messages to the Server for a given Client Socket
#[derive(Clone)]
pub struct PacketSender {
    message_port: MessagePort,
    server_addr: AddrCell,
}

impl PacketSender {
    /// Create a new PacketSender
    pub fn new(data_port: &DataPort, addr_cell: &AddrCell) -> Self {
        PacketSender {
            message_port: data_port.message_port(),
            server_addr: addr_cell.clone(),
        }
    }

    /// Send a Packet to the Server
    pub fn send(&self, payload: &[u8]) {
        let uarray: Uint8Array = payload.into();
        self.message_port
            .post_message(&uarray)
            .expect("Failed to send message");
    }

    /// Get the Server's Socket address
    pub fn server_addr(&self) -> ServerAddr {
        self.server_addr.get()
    }
}

unsafe impl Send for PacketSender {}
unsafe impl Sync for PacketSender {}
