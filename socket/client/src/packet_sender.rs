use super::{error::NaiaClientSocketError, server_addr::ServerAddr};

/// Used to send packets from the Client Socket
pub trait PacketSender: PacketSenderClone + Send + Sync {
    /// Sends a packet from the Client Socket
    fn send(&self, payload: &[u8]) -> Result<(), NaiaClientSocketError>;
    /// Get the Server's Socket address
    fn server_addr(&self) -> ServerAddr;
}

/// Used to clone Box<dyn PacketSender>
pub trait PacketSenderClone {
    /// Clone the boxed PacketSender
    fn clone_box(&self) -> Box<dyn PacketSender>;
}

impl<T: 'static + PacketSender + Clone> PacketSenderClone for T {
    fn clone_box(&self) -> Box<dyn PacketSender> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn PacketSender> {
    fn clone(&self) -> Box<dyn PacketSender> {
        PacketSenderClone::clone_box(self.as_ref())
    }
}
