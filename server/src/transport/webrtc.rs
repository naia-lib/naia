use std::net::SocketAddr;

use naia_shared::{IdentityToken, SocketConfig};

use naia_server_socket::{
    AuthReceiver, AuthSender, PacketReceiver, PacketSender, Socket as ServerSocket,
};

pub use naia_server_socket::ServerAddrs;

use super::{
    AuthReceiver as TransportAuthReceiver, AuthSender as TransportAuthSender, ListenResult,
    PacketReceiver as TransportReceiver, PacketSender as TransportSender, RecvError, SendError,
    Socket as TransportSocket,
};

#[doc(hidden)]
pub struct Socket {
    server_addrs: ServerAddrs,
    config: SocketConfig,
}

impl Socket {
    #[doc(hidden)]
    pub fn new(server_addrs: &ServerAddrs, config: &SocketConfig) -> Self {
        Self {
            server_addrs: server_addrs.clone(),
            config: config.clone(),
        }
    }
}

// Note: the socket-crate types below are concrete structs/enums whose inherent
// methods share names with the naia-level transport traits implemented here.
// Fully-qualified calls keep these from resolving back into themselves.
impl TransportSender for PacketSender {
    /// Sends a packet from the Server Socket
    fn send(&self, address: &SocketAddr, payload: &[u8]) -> Result<(), SendError> {
        PacketSender::send(self, address, payload).map_err(|_| SendError)
    }
}

impl TransportReceiver for PacketReceiver {
    /// Receives a packet from the Server Socket
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
        PacketReceiver::receive(self).map_err(|_| RecvError)
    }
}

impl TransportAuthSender for AuthSender {
    fn accept(
        &self,
        address: &SocketAddr,
        identity_token: &IdentityToken,
    ) -> Result<(), SendError> {
        AuthSender::accept(self, address, identity_token).map_err(|_| SendError)
    }
    fn reject(&self, address: &SocketAddr, payload: Option<&[u8]>) -> Result<(), SendError> {
        AuthSender::reject(self, address, payload).map_err(|_| SendError)
    }
}

impl TransportAuthReceiver for AuthReceiver {
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
        match AuthReceiver::receive(self) {
            Ok(auth_opt) => match auth_opt {
                Some((addr, payload)) => Ok(Some((addr, payload))),
                None => Ok(None),
            },
            Err(_err) => Err(RecvError),
        }
    }
}

impl From<Socket> for Box<dyn TransportSocket> {
    fn from(val: Socket) -> Self {
        Box::new(val)
    }
}

impl TransportSocket for Socket {
    fn listen(self: Box<Self>) -> ListenResult {
        let (inner_auth_sender, inner_auth_receiver, inner_packet_sender, inner_packet_receiver) =
            ServerSocket::listen_with_auth(&self.server_addrs, &self.config);
        (
            Box::new(inner_auth_sender),
            Box::new(inner_auth_receiver),
            Box::new(inner_packet_sender),
            Box::new(inner_packet_receiver),
        )
    }
}
