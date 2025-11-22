use crate::transport::{
    IdentityReceiver as TransportIdentityReceiver,
    IdentityReceiverResult as TransportIdentityReceiverResult, PacketReceiver as TransportReceiver,
    PacketSender as TransportSender, RecvError, SendError, ServerAddr as TransportServerAddr,
    Socket as TransportSocket,
};

use local_transport::{
    ClientIdentityReceiverResult, ClientRecvError, ClientSendError, ClientServerAddr,
    LocalClientIdentity, LocalClientReceiver, LocalClientSender, LocalClientSocket,
};

pub struct Socket {
    inner: Option<LocalClientSocket>,
}

impl Socket {
    pub fn new(local: LocalClientSocket) -> Self {
        Self { inner: Some(local) }
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
        let Socket { inner } = *self;
        let local_socket = inner.expect("local socket already taken");
        let (identity, sender, receiver) = local_socket.connect();
        (
            Box::new(LocalClientTransportIdentityReceiver(identity)),
            Box::new(LocalClientTransportSender(sender)),
            Box::new(LocalClientTransportReceiver(receiver)),
        )
    }

    fn connect_with_auth(
        self: Box<Self>,
        _auth_bytes: Vec<u8>,
    ) -> (
        Box<dyn TransportIdentityReceiver>,
        Box<dyn TransportSender>,
        Box<dyn TransportReceiver>,
    ) {
        self.connect()
    }

    fn connect_with_auth_headers(
        self: Box<Self>,
        _auth_headers: Vec<(String, String)>,
    ) -> (
        Box<dyn TransportIdentityReceiver>,
        Box<dyn TransportSender>,
        Box<dyn TransportReceiver>,
    ) {
        self.connect()
    }

    fn connect_with_auth_and_headers(
        self: Box<Self>,
        _auth_bytes: Vec<u8>,
        _auth_headers: Vec<(String, String)>,
    ) -> (
        Box<dyn TransportIdentityReceiver>,
        Box<dyn TransportSender>,
        Box<dyn TransportReceiver>,
    ) {
        self.connect()
    }
}

#[derive(Clone)]
struct LocalClientTransportSender(LocalClientSender);

impl TransportSender for LocalClientTransportSender {
    fn send(&self, payload: &[u8]) -> Result<(), SendError> {
        self.0.send(payload).map_err(|_| SendError)
    }

    fn server_addr(&self) -> TransportServerAddr {
        match self.0.server_addr() {
            ClientServerAddr::Found(addr) => TransportServerAddr::Found(addr),
            ClientServerAddr::Finding => TransportServerAddr::Finding,
        }
    }
}

#[derive(Clone)]
struct LocalClientTransportReceiver(LocalClientReceiver);

impl TransportReceiver for LocalClientTransportReceiver {
    fn receive(&mut self) -> Result<Option<&[u8]>, RecvError> {
        self.0.receive().map_err(|_| RecvError)
    }

    fn server_addr(&self) -> TransportServerAddr {
        match self.0.server_addr() {
            ClientServerAddr::Found(addr) => TransportServerAddr::Found(addr),
            ClientServerAddr::Finding => TransportServerAddr::Finding,
        }
    }
}

#[derive(Clone)]
struct LocalClientTransportIdentityReceiver(LocalClientIdentity);

impl TransportIdentityReceiver for LocalClientTransportIdentityReceiver {
    fn receive(&mut self) -> TransportIdentityReceiverResult {
        match self.0.receive() {
            ClientIdentityReceiverResult::Waiting => TransportIdentityReceiverResult::Waiting,
            ClientIdentityReceiverResult::Success(token) => {
                TransportIdentityReceiverResult::Success(token)
            }
            ClientIdentityReceiverResult::ErrorResponseCode(code) => {
                TransportIdentityReceiverResult::ErrorResponseCode(code)
            }
        }
    }
}
