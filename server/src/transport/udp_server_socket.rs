use naia_shared::LinkConditionerConfig;

use super::udp;

/// Server transport socket backed by UDP.
pub struct ServerSocket(udp::Socket);

impl ServerSocket {
    pub fn new(addrs: udp::ServerAddrs, config: Option<LinkConditionerConfig>) -> Self {
        Self(udp::Socket::new(&addrs, config))
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
