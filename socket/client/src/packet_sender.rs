use crate::error::NaiaClientSocketError;
use crate::ServerAddr;

/// Used to receive packets from the Server Socket
#[derive(Clone)]
pub struct PacketSender {
    inner: Box<dyn PacketSenderTrait>,
}

impl PacketSender {
    /// Create a new PacketReceiver
    pub fn new(inner: Box<dyn PacketSenderTrait>) -> Self {
        PacketSender { inner }
    }

    /// Receives a packet from the Server Socket
    pub fn send(&mut self, payload: &[u8]) -> Result<(), NaiaClientSocketError> {
        self.inner.send(payload)
    }

    /// Get the Server's Socket address
    pub fn server_addr(&self) -> ServerAddr {
        self.inner.server_addr()
    }
}


/// Used to send packets to the Server Socket
pub trait PacketSenderTrait: PacketSenderClone + Send + Sync {
    /// Receives a packet from the Server Socket
    fn send(&self, payload: &[u8]) -> Result<(), NaiaClientSocketError>;

    /// Get the Server's Socket address
    fn server_addr(&self) -> ServerAddr;
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
