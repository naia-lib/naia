mod server_addr;
pub use inner::{PacketReceiver, PacketSender, RecvError, SendError, Socket};
pub use server_addr::ServerAddr;

mod inner {

    use super::ServerAddr;

    pub struct SendError;

    pub struct RecvError;

    pub trait Socket {
        fn connect(self: Box<Self>) -> (Box<dyn PacketSender>, Box<dyn PacketReceiver>);
    }

    pub trait PacketSender {
        /// Sends a packet from the Client Socket
        fn send(&self, payload: &[u8]) -> Result<(), SendError>;
        /// Get the Server's Socket address
        fn server_addr(&self) -> ServerAddr;
    }

    pub trait PacketReceiver {
        /// Receives a packet from the Client Socket
        fn receive(&mut self) -> Result<Option<&[u8]>, RecvError>;
        /// Get the Server's Socket address
        fn server_addr(&self) -> ServerAddr;
    }
}
