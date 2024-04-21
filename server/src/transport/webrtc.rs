use std::net::SocketAddr;

use naia_shared::SocketConfig;

use naia_server_socket::{PacketReceiver, PacketSender, AuthReceiver, AuthSender, Socket as ServerSocket};

pub use naia_server_socket::ServerAddrs;

use super::{
    PacketReceiver as TransportReceiver, PacketSender as TransportSender, AuthSender as TransportAuthSender, AuthReceiver as TransportAuthReceiver, RecvError, SendError,
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

impl TransportSender for Box<dyn PacketSender> {
    /// Sends a packet from the Server Socket
    fn send(&self, address: &SocketAddr, payload: &[u8]) -> Result<(), SendError> {
        self.as_ref().send(address, payload).map_err(|_| SendError)
    }
}

impl TransportReceiver for Box<dyn PacketReceiver> {
    /// Receives a packet from the Server Socket
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
        self.as_mut().receive().map_err(|_| RecvError)
    }
}

impl TransportAuthSender for Box<dyn AuthSender> {
    ///
    fn accept(&self, address: &SocketAddr) -> Result<(), SendError> {
        self.as_ref().accept(address).map_err(|_| SendError)
    }
    ///
    fn reject(&self, address: &SocketAddr) -> Result<(), SendError> {
        self.as_ref().reject(address).map_err(|_| SendError)
    }
}

impl TransportAuthReceiver for Box<dyn AuthReceiver> {
    ///
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
        self.as_mut().receive().map_err(|_| RecvError)
    }
}

impl Into<Box<dyn TransportSocket>> for Socket {
    fn into(self) -> Box<dyn TransportSocket> {
        Box::new(self)
    }
}

impl TransportSocket for Socket {
    fn listen(self: Box<Self>) -> (Box<dyn TransportAuthSender>, Box<dyn TransportAuthReceiver>, Box<dyn TransportSender>, Box<dyn TransportReceiver>) {
        let (
            inner_auth_sender,
            inner_auth_receiver,
            inner_packet_sender,
            inner_packet_receiver
        ) = ServerSocket::listen_with_auth(&self.server_addrs, &self.config);
        return (
            Box::new(inner_auth_sender),
            Box::new(inner_auth_receiver),
            Box::new(inner_packet_sender),
            Box::new(inner_packet_receiver)
        );
    }
}
