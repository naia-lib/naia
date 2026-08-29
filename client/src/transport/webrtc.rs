use naia_shared::SocketConfig;

use naia_client_socket::{
    IdentityReceiver, IdentityReceiverResult as SocketIdentityReceiverResult, PacketReceiver,
    PacketSender, ServerAddr, Socket as ClientSocket,
};

use super::{
    IdentityReceiver as TransportIdentityReceiver, IdentityReceiverResult,
    PacketReceiver as TransportReceiver, PacketSender as TransportSender, RecvError, SendError,
    ServerAddr as TransportAddr, Socket as TransportSocket,
};

#[doc(hidden)]
pub struct Socket {
    server_session_url: String,
    config: SocketConfig,
}

impl Socket {
    #[doc(hidden)]
    pub fn new(server_session_url: &str, config: &SocketConfig) -> Self {
        Self {
            server_session_url: server_session_url.to_string(),
            config: config.clone(),
        }
    }
}

// Note: the socket-crate types below are concrete structs/enums whose inherent
// methods share names with the naia-level transport traits implemented here.
// Fully-qualified calls keep these from resolving back into themselves.
impl TransportSender for PacketSender {
    /// Sends a packet from the Client Socket
    fn send(&self, payload: &[u8]) -> Result<(), SendError> {
        PacketSender::send(self, payload).map_err(|_| SendError)
    }
    /// Get the Server's Socket address
    fn server_addr(&self) -> TransportAddr {
        match PacketSender::server_addr(self) {
            ServerAddr::Found(addr) => TransportAddr::Found(addr),
            ServerAddr::Finding => TransportAddr::Finding,
        }
    }
}

impl TransportReceiver for PacketReceiver {
    /// Receives a packet from the Client Socket
    fn receive(&mut self) -> Result<Option<&[u8]>, RecvError> {
        PacketReceiver::receive(self).map_err(|_| RecvError)
    }
    /// Get the Server's Socket address
    fn server_addr(&self) -> TransportAddr {
        match PacketReceiver::server_addr(self) {
            ServerAddr::Found(addr) => TransportAddr::Found(addr),
            ServerAddr::Finding => TransportAddr::Finding,
        }
    }
}

impl TransportIdentityReceiver for IdentityReceiver {
    /// Receives an IdentityToken from the Client Socket
    fn receive(&mut self) -> IdentityReceiverResult {
        match IdentityReceiver::receive(self) {
            SocketIdentityReceiverResult::Waiting => IdentityReceiverResult::Waiting,
            SocketIdentityReceiverResult::Success(token) => IdentityReceiverResult::Success(token),
            SocketIdentityReceiverResult::ErrorResponseCode(code, payload) => {
                IdentityReceiverResult::ErrorResponseCode(code, payload)
            }
        }
    }
}

impl From<Socket> for Box<dyn TransportSocket> {
    fn from(val: Socket) -> Self {
        Box::new(val)
    }
}

impl TransportSocket for Socket {
    fn connect(
        self: Box<Self>,
    ) -> (
        Box<dyn TransportIdentityReceiver>,
        Box<dyn TransportSender>,
        Box<dyn TransportReceiver>,
    ) {
        let (id_receiver, inner_sender, inner_receiver) =
            ClientSocket::connect(&self.server_session_url, &self.config);
        (
            Box::new(id_receiver),
            Box::new(inner_sender),
            Box::new(inner_receiver),
        )
    }
    fn connect_with_auth(
        self: Box<Self>,
        auth_bytes: Vec<u8>,
    ) -> (
        Box<dyn TransportIdentityReceiver>,
        Box<dyn TransportSender>,
        Box<dyn TransportReceiver>,
    ) {
        let (id_receiver, inner_sender, inner_receiver) =
            ClientSocket::connect_with_auth(&self.server_session_url, &self.config, auth_bytes);
        (
            Box::new(id_receiver),
            Box::new(inner_sender),
            Box::new(inner_receiver),
        )
    }
    fn connect_with_auth_headers(
        self: Box<Self>,
        auth_headers: Vec<(String, String)>,
    ) -> (
        Box<dyn TransportIdentityReceiver>,
        Box<dyn TransportSender>,
        Box<dyn TransportReceiver>,
    ) {
        let (id_receiver, inner_sender, inner_receiver) = ClientSocket::connect_with_auth_headers(
            &self.server_session_url,
            &self.config,
            auth_headers,
        );
        (
            Box::new(id_receiver),
            Box::new(inner_sender),
            Box::new(inner_receiver),
        )
    }
    fn connect_with_auth_and_headers(
        self: Box<Self>,
        auth_bytes: Vec<u8>,
        auth_headers: Vec<(String, String)>,
    ) -> (
        Box<dyn TransportIdentityReceiver>,
        Box<dyn TransportSender>,
        Box<dyn TransportReceiver>,
    ) {
        let (id_receiver, inner_sender, inner_receiver) =
            ClientSocket::connect_with_auth_and_headers(
                &self.server_session_url,
                &self.config,
                auth_bytes,
                auth_headers,
            );
        (
            Box::new(id_receiver),
            Box::new(inner_sender),
            Box::new(inner_receiver),
        )
    }
}
