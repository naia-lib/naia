use std::net::SocketAddr;

use crate::NaiaServerSocketError;


/// Used to send packets to the Server Socket
#[derive(Clone)]
pub struct PacketSender {
    inner: Box<dyn PacketSenderTrait>,
}

impl PacketSender {
    /// Create a new PacketSender
    pub fn new(inner: Box<dyn PacketSenderTrait>) -> Self {
        PacketSender { inner }
    }

    /// Sends a packet to the Server Socket
    pub fn send(&mut self, address: &SocketAddr, payload: &[u8]) -> Result<(), NaiaServerSocketError> {
        self.inner.send(address, payload)
    }
}


/// Used to send packets to the Server Socket
pub trait PacketSenderTrait: PacketSenderClone + Send + Sync {
    /// Sends a packet to the Server Socket
    fn send(&self, address: &SocketAddr, payload: &[u8]) -> Result<(), NaiaServerSocketError>;
}

/// Used to clone Box<dyn PacketSenderTrait>
pub trait PacketSenderClone {
    /// Clone the boxed PacketSender
    fn clone_box(&self) -> Box<dyn PacketSenderTrait>;
}

impl<T: 'static + PacketSenderTrait + Clone> PacketSenderClone for T {
    fn clone_box(&self) -> Box<dyn PacketSenderTrait> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn PacketSenderTrait> {
    fn clone(&self) -> Box<dyn PacketSenderTrait> {
        PacketSenderClone::clone_box(self.as_ref())
    }
}