pub use inner::{PacketReceiver, PacketSender, RecvError, SendError, Socket};

mod inner {

    use std::net::SocketAddr;

    pub struct SendError;

    pub struct RecvError;

    pub trait Socket {
        fn listen(self: Box<Self>) -> (Box<dyn PacketSender>, Box<dyn PacketReceiver>);
    }

    pub trait PacketSender {
        /// Sends a packet to the Server Socket
        fn send(&self, address: &SocketAddr, payload: &[u8]) -> Result<(), SendError>;
    }

    pub trait PacketReceiver {
        /// Receives a packet from the Server Socket
        fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError>;
    }
}
