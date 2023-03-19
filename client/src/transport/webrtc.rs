use naia_shared::SocketConfig;

use naia_client_socket::{PacketReceiver, PacketSender, ServerAddr, Socket};

use super::{
    PacketReceiver as TransportReceiver, PacketSender as TransportSender, RecvError, SendError,
    ServerAddr as TransportAddr, Socket as TransportSocket,
};

pub struct WebRTCSocket {
    server_session_url: String,
    config: SocketConfig,
}

impl WebRTCSocket {
    pub fn new(server_session_url: &str, config: &SocketConfig) -> Self {
        return Self {
            server_session_url: server_session_url.to_string(),
            config: config.clone(),
        };
    }
}

impl TransportSender for PacketSender {
    /// Sends a packet from the Client Socket
    fn send(&self, payload: &[u8]) -> Result<(), SendError> {
        self.send(payload).map_err(|_| SendError)
    }
    /// Get the Server's Socket address
    fn server_addr(&self) -> TransportAddr {
        match self.server_addr() {
            ServerAddr::Found(addr) => TransportAddr::Found(addr),
            ServerAddr::Finding => TransportAddr::Finding,
        }
    }
}

impl TransportReceiver for PacketReceiver {
    /// Receives a packet from the Client Socket
    fn receive(&mut self) -> Result<Option<&[u8]>, RecvError> {
        self.receive().map_err(|_| RecvError)
    }
    /// Get the Server's Socket address
    fn server_addr(&self) -> TransportAddr {
        match self.server_addr() {
            ServerAddr::Found(addr) => TransportAddr::Found(addr),
            ServerAddr::Finding => TransportAddr::Finding,
        }
    }
}

impl Into<Box<dyn TransportSocket>> for WebRTCSocket {
    fn into(self) -> Box<dyn TransportSocket> {
        Box::new(self)
    }
}

impl TransportSocket for WebRTCSocket {
    fn connect(self: Box<Self>) -> (Box<dyn TransportSender>, Box<dyn TransportReceiver>) {
        let (inner_sender, inner_receiver) =
            Socket::connect(&self.server_session_url, &self.config);
        return (Box::new(inner_sender), Box::new(inner_receiver));
    }
}
