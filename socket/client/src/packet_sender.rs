use super::{error::NaiaClientSocketError, server_addr::ServerAddr};

// Impl

/// Used to send packets from the Client Socket
pub struct PacketSender {
    inner: Box<dyn PacketSenderTrait>,
}

impl PacketSender {
    /// Create a new PacketSender
    pub fn new(inner: Box<dyn PacketSenderTrait>) -> Self {
        PacketSender { inner }
    }

    /// Sends a packet from the Client Socket
    pub fn send(&self, payload: &[u8]) -> Result<(), NaiaClientSocketError> {
        return self.inner.send(payload);
    }
    /// Get the Server's Socket address
    pub fn server_addr(&self) -> ServerAddr {
        return self.inner.server_addr();
    }
}

// Trait

/// Used to send packets from the Client Socket
pub trait PacketSenderTrait: Send + Sync {
    /// Sends a packet from the Client Socket
    fn send(&self, payload: &[u8]) -> Result<(), NaiaClientSocketError>;
    /// Get the Server's Socket address
    fn server_addr(&self) -> ServerAddr;
}
