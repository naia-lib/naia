use js_sys::Uint8Array;
use web_sys::MessagePort;

use crate::{
    error::NaiaClientSocketError, packet_sender::PacketSender, server_addr::ServerAddr,
};

use super::{addr_cell::AddrCell, data_port::DataPort};

/// Handles sending messages to the Server for a given Client Socket
#[derive(Clone)]
pub struct PacketSenderImpl {
    message_port: MessagePort,
    server_addr: AddrCell,
}

impl PacketSenderImpl {
    /// Create a new PacketSender
    pub fn new(data_port: &DataPort, addr_cell: &AddrCell) -> Self {
        PacketSenderImpl {
            message_port: data_port.message_port(),
            server_addr: addr_cell.clone(),
        }
    }
}

impl PacketSender for PacketSenderImpl {
    /// Send a Packet to the Server
    fn send(&self, payload: &[u8]) -> Result<(), NaiaClientSocketError> {
        let uarray: Uint8Array = payload.into();
        self.message_port
            .post_message(&uarray)
            .expect("Failed to send message");
        Ok(())
    }

    /// Get the Server's Socket address
    fn server_addr(&self) -> ServerAddr {
        self.server_addr.get()
    }
}

unsafe impl Send for PacketSenderImpl {}
unsafe impl Sync for PacketSenderImpl {}
