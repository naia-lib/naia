use std::net::SocketAddr;

use naia_shared::LinkConditionerConfig;

use crate::transport::{
    AuthReceiver as TransportAuthReceiver, AuthSender as TransportAuthSender,
    ConditionedPacketReceiver, PacketReceiver as TransportReceiver, PacketSender as TransportSender, RecvError, SendError,
    Socket as TransportSocket,
};

use super::{
    LocalServerAuthReceiver, LocalServerAuthSender, LocalServerReceiver, LocalServerSender,
    LocalServerSocket,
};

pub struct Socket {
    inner: Option<LocalServerSocket>,
    config: Option<LinkConditionerConfig>,
}

impl Socket {
    pub fn new(local: LocalServerSocket, config: Option<LinkConditionerConfig>) -> Self {
        Self { inner: Some(local), config }
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
        let Socket { inner, config } = *self;
        let local = inner.expect("server socket already taken");
        let (auth_sender, auth_receiver, sender, receiver) = local.listen_with_auth();

        let receiver: Box<dyn TransportReceiver> = {
            let wrapped = LocalServerTransportReceiver(receiver);
            if let Some(config) = &config {
                Box::new(ConditionedPacketReceiver::new(Box::new(wrapped), config))
            } else {
                Box::new(wrapped)
            }
        };

        (
            Box::new(LocalServerTransportAuthSender(auth_sender)),
            Box::new(LocalServerTransportAuthReceiver(auth_receiver)),
            Box::new(LocalServerTransportSender(sender)),
            receiver,
        )
    }
}

#[derive(Clone)]
struct LocalServerTransportSender(LocalServerSender);

impl TransportSender for LocalServerTransportSender {
    fn send(&self, address: &SocketAddr, payload: &[u8]) -> Result<(), SendError> {
        self.0.send(address, payload).map_err(|_| SendError)
    }
}

#[derive(Clone)]
struct LocalServerTransportReceiver(LocalServerReceiver);

impl TransportReceiver for LocalServerTransportReceiver {
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
        self.0.receive().map_err(|_| RecvError)
    }
}

#[derive(Clone)]
struct LocalServerTransportAuthSender(LocalServerAuthSender);

impl TransportAuthSender for LocalServerTransportAuthSender {
    fn accept(
        &self,
        address: &SocketAddr,
        identity_token: &naia_shared::IdentityToken,
    ) -> Result<(), SendError> {
        self.0.accept(address, identity_token).map_err(|_| SendError)
    }

    fn reject(&self, address: &SocketAddr) -> Result<(), SendError> {
        self.0.reject(address).map_err(|_| SendError)
    }
}

#[derive(Clone)]
struct LocalServerTransportAuthReceiver(LocalServerAuthReceiver);

impl TransportAuthReceiver for LocalServerTransportAuthReceiver {
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
        self.0.receive().map_err(|_| RecvError)
    }
}