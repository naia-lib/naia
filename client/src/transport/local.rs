use crate::transport::{
    ConditionedPacketReceiver, IdentityReceiver as TransportIdentityReceiver,
    IdentityReceiverResult as TransportIdentityReceiverResult, PacketReceiver as TransportReceiver,
    PacketSender as TransportSender, RecvError, SendError, ServerAddr as TransportServerAddr,
    Socket as TransportSocket,
};

use local_transport_client::{
    ClientIdentityReceiverResult, ClientServerAddr,
    LocalClientIdentity, LocalClientReceiver, LocalClientSender, LocalClientSocket,
};

use naia_shared::LinkConditionerConfig;

pub struct Socket {
    inner: Option<LocalClientSocket>,
    config: Option<LinkConditionerConfig>,
}

impl Socket {
    pub fn new(local: LocalClientSocket, config: Option<LinkConditionerConfig>) -> Self {
        Self { inner: Some(local), config }
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
        let Socket { inner, config } = *self;
        let local_socket = inner.expect("local socket already taken");
        let (identity, sender, receiver) = local_socket.connect();
        
        let receiver: Box<dyn TransportReceiver> = {
            let wrapped = LocalClientTransportReceiver(receiver);
            if let Some(config) = &config {
                Box::new(ConditionedPacketReceiver::new(Box::new(wrapped), config))
            } else {
                Box::new(wrapped)
            }
        };
        
        (
            Box::new(LocalClientTransportIdentityReceiver(identity)),
            Box::new(LocalClientTransportSender(sender)),
            receiver,
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
        let Socket { inner, config } = *self;
        let local_socket = inner.expect("local socket already taken");
        let (identity, sender, receiver) = local_socket.connect_with_auth(auth_bytes);
        
        let receiver: Box<dyn TransportReceiver> = {
            let wrapped = LocalClientTransportReceiver(receiver);
            if let Some(config) = &config {
                Box::new(ConditionedPacketReceiver::new(Box::new(wrapped), config))
            } else {
                Box::new(wrapped)
            }
        };
        
        (
            Box::new(LocalClientTransportIdentityReceiver(identity)),
            Box::new(LocalClientTransportSender(sender)),
            receiver,
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
        let Socket { inner, config } = *self;
        let local_socket = inner.expect("local socket already taken");
        let (identity, sender, receiver) = local_socket.connect_with_auth_headers(auth_headers);
        
        let receiver: Box<dyn TransportReceiver> = {
            let wrapped = LocalClientTransportReceiver(receiver);
            if let Some(config) = &config {
                Box::new(ConditionedPacketReceiver::new(Box::new(wrapped), config))
            } else {
                Box::new(wrapped)
            }
        };
        
        (
            Box::new(LocalClientTransportIdentityReceiver(identity)),
            Box::new(LocalClientTransportSender(sender)),
            receiver,
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
        let Socket { inner, config } = *self;
        let local_socket = inner.expect("local socket already taken");
        let (identity, sender, receiver) = local_socket.connect_with_auth_and_headers(auth_bytes, auth_headers);
        
        let receiver: Box<dyn TransportReceiver> = {
            let wrapped = LocalClientTransportReceiver(receiver);
            if let Some(config) = &config {
                Box::new(ConditionedPacketReceiver::new(Box::new(wrapped), config))
            } else {
                Box::new(wrapped)
            }
        };
        
        (
            Box::new(LocalClientTransportIdentityReceiver(identity)),
            Box::new(LocalClientTransportSender(sender)),
            receiver,
        )
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
