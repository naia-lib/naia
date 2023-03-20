use super::{error::NaiaClientSocketError, server_addr::ServerAddr};

/// Used to receive packets from the Client Socket
pub trait PacketReceiver: PacketReceiverClone + Send + Sync {
    /// Receives a packet from the Client Socket
    fn receive(&mut self) -> Result<Option<&[u8]>, NaiaClientSocketError>;
    /// Get the Server's Socket address
    fn server_addr(&self) -> ServerAddr;
}

/// Used to clone Box<dyn PacketReceiver>
pub trait PacketReceiverClone {
    /// Clone the boxed PacketReceiver
    fn clone_box(&self) -> Box<dyn PacketReceiver>;
}

impl<T: 'static + PacketReceiver + Clone> PacketReceiverClone for T {
    fn clone_box(&self) -> Box<dyn PacketReceiver> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn PacketReceiver> {
    fn clone(&self) -> Box<dyn PacketReceiver> {
        PacketReceiverClone::clone_box(self.as_ref())
    }
}