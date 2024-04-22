use std::net::SocketAddr;

use smol::channel::{Sender, TrySendError};

use naia_socket_shared::IdentityToken;

use crate::NaiaServerSocketError;

// Trait
pub trait AuthSender: AuthSenderClone + Send + Sync {
    /// Accepts an incoming connection on the Server Socket
    fn accept(
        &self,
        address: &SocketAddr,
        identity_token: &IdentityToken,
    ) -> Result<(), NaiaServerSocketError>;
    /// Rejects an incoming connection from the Server Socket
    fn reject(&self, address: &SocketAddr) -> Result<(), NaiaServerSocketError>;
}

// Impl
/// Used to send Auth messages to the Server Socket
#[derive(Clone)]
pub struct AuthSenderImpl {
    channel_sender: Sender<(SocketAddr, Option<IdentityToken>)>,
}

impl AuthSenderImpl {
    /// Creates a new AuthSender
    pub fn new(channel_sender: Sender<(SocketAddr, Option<IdentityToken>)>) -> Self {
        Self { channel_sender }
    }

    fn send(
        &self,
        address: &SocketAddr,
        accept: Option<IdentityToken>,
    ) -> Result<(), NaiaServerSocketError> {
        self.channel_sender
            .try_send((*address, accept))
            .map_err(|err| match err {
                TrySendError::Full(_) => unreachable!("the channel is expected to be unbound"),
                TrySendError::Closed(_) => NaiaServerSocketError::SendError(*address),
            })
    }
}

impl AuthSender for AuthSenderImpl {
    /// Accepts an incoming connection on the Server Socket
    fn accept(
        &self,
        address: &SocketAddr,
        identity_token: &IdentityToken,
    ) -> Result<(), NaiaServerSocketError> {
        self.send(address, Some(identity_token.clone()))
    }

    /// Rejects an incoming connection from the Server Socket
    fn reject(&self, address: &SocketAddr) -> Result<(), NaiaServerSocketError> {
        self.send(address, None)
    }
}

/// Used to clone Box<dyn AuthSender>
pub trait AuthSenderClone {
    /// Clone the boxed AuthSender
    fn clone_box(&self) -> Box<dyn AuthSender>;
}

impl<T: 'static + AuthSender + Clone> AuthSenderClone for T {
    fn clone_box(&self) -> Box<dyn AuthSender> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn AuthSender> {
    fn clone(&self) -> Box<dyn AuthSender> {
        AuthSenderClone::clone_box(self.as_ref())
    }
}
