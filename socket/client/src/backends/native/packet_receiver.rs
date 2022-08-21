
use crossbeam::channel::Receiver;

use crate::{
    error::NaiaClientSocketError, packet_receiver::PacketReceiverTrait, server_addr::ServerAddr,
};
use crate::backends::native::addr_cell::AddrCell;

/// Handles receiving messages from the Server through a given Client Socket
#[derive(Clone)]
pub struct PacketReceiverImpl {
    server_addr: AddrCell,
    receiver_channel: Receiver<Box<[u8]>>,
    receive_buffer: Vec<u8>,
}

impl PacketReceiverImpl {
    /// Create a new PacketReceiver, if supplied with the Server's address & a
    /// reference back to the parent Socket
    pub fn new(server_addr: AddrCell, receiver_channel: Receiver<Box<[u8]>>) -> Self {
        PacketReceiverImpl {
            server_addr,
            receiver_channel,
            receive_buffer: vec![0; 1472],
        }
    }
}

impl PacketReceiverTrait for PacketReceiverImpl {
    fn receive(&mut self) -> Result<Option<&[u8]>, NaiaClientSocketError> {

        if let Ok(bytes) = self.receiver_channel.recv() {
            let length = bytes.len();
            self.receive_buffer.clone_from_slice(&bytes);
            return Ok(Some(&self.receive_buffer[..length]));
        }

        return Ok(None);
    }

    /// Get the Server's Socket address
    fn server_addr(&self) -> ServerAddr {
        return self.server_addr.get();
    }
}
