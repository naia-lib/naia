use std::net::SocketAddr;

use smol::channel::{Sender, TrySendError};

use naia_socket_shared::IdentityToken;

use crate::NaiaServerSocketError;

/// Used to send Auth messages to the Server Socket
#[derive(Clone)]
pub struct AuthSender {
    channel_sender: Sender<(SocketAddr, Option<IdentityToken>)>,
}

impl AuthSender {
    /// Creates a new AuthSender
    pub fn new(channel_sender: Sender<(SocketAddr, Option<IdentityToken>)>) -> Self {
        Self { channel_sender }
    }

    /// Accepts an incoming connection on the Server Socket
    pub fn accept(
        &self,
        address: &SocketAddr,
        identity_token: &IdentityToken,
    ) -> Result<(), NaiaServerSocketError> {
        self.send(address, Some(identity_token.clone()))
    }

    /// Rejects an incoming connection from the Server Socket
    pub fn reject(&self, address: &SocketAddr) -> Result<(), NaiaServerSocketError> {
        self.send(address, None)
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
