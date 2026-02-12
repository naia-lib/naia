use super::local::{LocalServerSocket, Socket};

/// Server transport socket backed by in-memory channels (for tests).
pub struct ServerSocket(Socket);

impl ServerSocket {
    pub fn new(hub: super::local::LocalTransportHub) -> Self {
        Self(Socket::new(LocalServerSocket::new(hub), None))
    }
}

impl Into<Box<dyn super::Socket>> for ServerSocket {
    fn into(self) -> Box<dyn super::Socket> {
        Box::new(self)
    }
}

impl super::Socket for ServerSocket {
    fn listen(
        self: Box<Self>,
    ) -> (
        Box<dyn super::AuthSender>,
        Box<dyn super::AuthReceiver>,
        Box<dyn super::PacketSender>,
        Box<dyn super::PacketReceiver>,
    ) {
        Box::new(self.0).listen()
    }
}
