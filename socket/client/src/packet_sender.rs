use super::{error::NaiaClientSocketError, server_addr::ServerAddr};

// Impl

/// Used to send packets from the Client Socket
pub trait PacketSender: Send + Sync {
    /// Sends a packet from the Client Socket
    fn send(&self, payload: &[u8]) -> Result<(), NaiaClientSocketError>;
    /// Get the Server's Socket address
    fn server_addr(&self) -> ServerAddr;
}
