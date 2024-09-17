use naia_shared::SocketConfig;

use naia_client_socket::{
    IdentityReceiver, IdentityReceiverResult, PacketReceiver, PacketSender, ServerAddr,
    Socket as ClientSocket,
};

use super::{
    IdentityReceiver as TransportIdentityReceiver, PacketReceiver as TransportReceiver,
    PacketSender as TransportSender, RecvError, SendError, ServerAddr as TransportAddr,
    Socket as TransportSocket,
};

pub struct Socket {
    server_session_url: String,
    config: SocketConfig,
}

impl Socket {
    pub fn new(server_session_url: &str, config: &SocketConfig) -> Self {
        return Self {
            server_session_url: server_session_url.to_string(),
            config: config.clone(),
        };
    }
}

impl TransportSender for Box<dyn PacketSender> {
    /// Sends a packet from the Client Socket
    fn send(&self, payload: &[u8]) -> Result<(), SendError> {
        self.as_ref().send(payload).map_err(|_| SendError)
    }
    /// Get the Server's Socket address
    fn server_addr(&self) -> TransportAddr {
        match self.as_ref().server_addr() {
            ServerAddr::Found(addr) => TransportAddr::Found(addr),
            ServerAddr::Finding => TransportAddr::Finding,
        }
    }
}

impl TransportReceiver for Box<dyn PacketReceiver> {
    /// Receives a packet from the Client Socket
    fn receive(&mut self) -> Result<Option<&[u8]>, RecvError> {
        self.as_mut().receive().map_err(|_| RecvError)
    }
    /// Get the Server's Socket address
    fn server_addr(&self) -> TransportAddr {
        match self.as_ref().server_addr() {
            ServerAddr::Found(addr) => TransportAddr::Found(addr),
            ServerAddr::Finding => TransportAddr::Finding,
        }
    }
}

impl TransportIdentityReceiver for Box<dyn IdentityReceiver> {
    /// Receives an IdentityToken from the Client Socket
    fn receive(&mut self) -> IdentityReceiverResult {
        self.as_mut().receive()
    }
}

impl Into<Box<dyn TransportSocket>> for Socket {
    fn into(self) -> Box<dyn TransportSocket> {
        Box::new(self)
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
        return (
            Box::new(id_receiver),
            Box::new(inner_sender),
            Box::new(inner_receiver),
        );
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
        return (
            Box::new(id_receiver),
            Box::new(inner_sender),
            Box::new(inner_receiver),
        );
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
        return (
            Box::new(id_receiver),
            Box::new(inner_sender),
            Box::new(inner_receiver),
        );
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
        return (
            Box::new(id_receiver),
            Box::new(inner_sender),
            Box::new(inner_receiver),
        );
    }
}
