use super::{error::NaiaClientSocketError, server_addr::ServerAddr};

// Impl

/// Used to receive packets from the Client Socket
pub struct PacketReceiver {
    inner: Box<dyn PacketReceiverTrait>,
}

impl PacketReceiver {
    /// Create a new PacketReceiver
    pub fn new(inner: Box<dyn PacketReceiverTrait>) -> Self {
        PacketReceiver { inner }
    }

    /// Receives a packet from the Client Socket
    pub fn receive(&mut self) -> Result<Option<&[u8]>, NaiaClientSocketError> {
        self.inner.receive()
    }

    /// Get the Server's Socket address
    pub fn server_addr(&self) -> ServerAddr {
        self.inner.server_addr()
    }
}

// Trait

/// Used to receive packets from the Client Socket
pub trait PacketReceiverTrait: Send + Sync {
    /// Receives a packet from the Client Socket
    fn receive(&mut self) -> Result<Option<&[u8]>, NaiaClientSocketError>;
    /// Get the Server's Socket address
    fn server_addr(&self) -> ServerAddr;
}