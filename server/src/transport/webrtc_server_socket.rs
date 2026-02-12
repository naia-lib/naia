use naia_shared::SocketConfig;

use super::webrtc;

/// Server transport socket backed by WebRTC.
pub struct ServerSocket(webrtc::Socket);

impl ServerSocket {
    pub fn new(addrs: webrtc::ServerAddrs, config: SocketConfig) -> Self {
        Self(webrtc::Socket::new(&addrs, &config))
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
