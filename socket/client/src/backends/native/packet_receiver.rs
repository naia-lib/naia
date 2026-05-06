use std::sync::{Arc, Mutex};

use tokio::sync::mpsc::UnboundedReceiver;
use webrtc_unreliable_client::{AddrCell, ServerAddr as RTCServerAddr};

use crate::{
    error::NaiaClientSocketError, packet_receiver::PacketReceiver, server_addr::ServerAddr,
};

/// Handles receiving messages from the Server through a given Client Socket
#[derive(Clone)]
pub struct PacketReceiverImpl {
    server_addr: AddrCell,
    receiver_channel: Arc<Mutex<UnboundedReceiver<Box<[u8]>>>>,
    receive_buffer: Vec<u8>,
}

impl PacketReceiverImpl {
    /// Create a new PacketReceiver, if supplied with the Server's address & a
    /// reference back to the parent Socket
    pub fn new(server_addr: AddrCell, receiver_channel: UnboundedReceiver<Box<[u8]>>) -> Self {
        PacketReceiverImpl {
            server_addr,
            receiver_channel: Arc::new(Mutex::new(receiver_channel)),
            receive_buffer: vec![0; 1472],
        }
    }
}

impl PacketReceiver for PacketReceiverImpl {
    fn receive(&mut self) -> Result<Option<&[u8]>, NaiaClientSocketError> {
        if let Ok(mut receiver) = self.receiver_channel.lock() {
            if let Ok(bytes) = receiver.try_recv() {
                let length = bytes.len();
                self.receive_buffer[..length].clone_from_slice(&bytes);
                return Ok(Some(&self.receive_buffer[..length]));
            }
        }
        Ok(None)
    }

    /// Get the Server's Socket address
    fn server_addr(&self) -> ServerAddr {
        match self.server_addr.get() {
            RTCServerAddr::Finding => ServerAddr::Finding,
            RTCServerAddr::Found(addr) => ServerAddr::Found(addr),
        }
    }
}
