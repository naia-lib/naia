use std::net::SocketAddr;

use naia_shared::{IdentityToken, SocketConfig};

use naia_server_socket::{AuthReceiver, AuthSender, PacketReceiver, PacketSender, Socket as ServerSocket};

pub use naia_server_socket::ServerAddrs;

use crate::user::UserAuthAddr;
use super::{
    AuthReceiver as TransportAuthReceiver, AuthSender as TransportAuthSender,
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
    fn accept(
        &self,
        address: &UserAuthAddr,
        identity_token: &IdentityToken,
    ) -> Result<(), SendError> {
        self.as_ref()
            .accept(&address.addr(), identity_token)
            .map_err(|_| SendError)
    }
    ///
    fn reject(&self, address: &UserAuthAddr) -> Result<(), SendError> {
        self.as_ref().reject(&address.addr()).map_err(|_| SendError)
    }
}

impl TransportAuthReceiver for Box<dyn AuthReceiver> {
    ///
    fn receive(&mut self) -> Result<Option<(UserAuthAddr, &[u8])>, RecvError> {
        match self.as_mut().receive() {
            Ok(auth_opt) => {
                match auth_opt {
                    Some((addr, payload)) => {
                        return Ok(Some((UserAuthAddr::new(addr), payload)));
                    }
                    None => { return Ok(None); }
                }
            }
            Err(_err) => { return Err(RecvError); }
        }
    }
}

impl Into<Box<dyn TransportSocket>> for Socket {
    fn into(self) -> Box<dyn TransportSocket> {
        Box::new(self)
    }
}

impl TransportSocket for Socket {
    fn listen(
        self: Box<Self>,
    ) -> (
        Box<dyn TransportAuthSender>,
        Box<dyn TransportAuthReceiver>,
        Box<dyn TransportSender>,
        Box<dyn TransportReceiver>,
    ) {
        let (inner_auth_sender, inner_auth_receiver, inner_packet_sender, inner_packet_receiver) =
            ServerSocket::listen_with_auth(&self.server_addrs, &self.config);
        return (
            Box::new(inner_auth_sender),
            Box::new(inner_auth_receiver),
            Box::new(inner_packet_sender),
            Box::new(inner_packet_receiver),
        );
    }
}
