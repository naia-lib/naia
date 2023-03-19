use std::net::SocketAddr;

use naia_shared::SocketConfig;

use naia_server_socket::{PacketReceiver, PacketSender, Socket as ServerSocket};

pub use naia_server_socket::ServerAddrs;

use super::{
    PacketReceiver as TransportReceiver, PacketSender as TransportSender, RecvError, SendError,
    Socket as TransportSocket,
};

pub struct Socket {
    server_addrs: ServerAddrs,
    config: SocketConfig,
}

impl Socket {
    pub fn new(server_addrs: &ServerAddrs, config: &SocketConfig) -> Self {
        return Self {
            server_addrs: server_addrs.clone(),
            config: config.clone(),
        };
    }
}

impl TransportSender for PacketSender {
    /// Sends a packet from the Server Socket
    fn send(&self, address: &SocketAddr, payload: &[u8]) -> Result<(), SendError> {
        self.send(address, payload).map_err(|_| SendError)
    }
}

impl TransportReceiver for PacketReceiver {
    /// Receives a packet from the Server Socket
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
        self.receive().map_err(|_| RecvError)
    }
}

impl Into<Box<dyn TransportSocket>> for Socket {
    fn into(self) -> Box<dyn TransportSocket> {
        Box::new(self)
    }
}

impl TransportSocket for Socket {
    fn listen(self: Box<Self>) -> (Box<dyn TransportSender>, Box<dyn TransportReceiver>) {
        let (inner_sender, inner_receiver) = ServerSocket::listen(&self.server_addrs, &self.config);
        return (Box::new(inner_sender), Box::new(inner_receiver));
    }
}
