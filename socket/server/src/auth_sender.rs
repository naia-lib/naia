use std::net::SocketAddr;

use smol::channel::{Sender, TrySendError};

use naia_socket_shared::IdentityToken;

use crate::{AuthResponse, NaiaServerSocketError};

/// Used to send Auth messages to the Server Socket
#[derive(Clone)]
pub struct AuthSender {
    channel_sender: Sender<(SocketAddr, AuthResponse)>,
}

impl AuthSender {
    /// Creates a new AuthSender
    pub fn new(channel_sender: Sender<(SocketAddr, AuthResponse)>) -> Self {
        Self { channel_sender }
    }

    /// Accepts an incoming connection on the Server Socket
    pub fn accept(
        &self,
        address: &SocketAddr,
        identity_token: &IdentityToken,
    ) -> Result<(), NaiaServerSocketError> {
        self.send(address, AuthResponse::Accept(identity_token.clone()))
    }

    /// Rejects an incoming connection from the Server Socket
    ///
    /// `payload`, when present, is an already-serialized message telling the
    /// client why it was rejected (naia-lib/naia#133).
    pub fn reject(
        &self,
        address: &SocketAddr,
        payload: Option<&[u8]>,
    ) -> Result<(), NaiaServerSocketError> {
        self.send(address, AuthResponse::Reject(payload.map(|p| p.to_vec())))
    }

    fn send(
        &self,
        address: &SocketAddr,
        response: AuthResponse,
    ) -> Result<(), NaiaServerSocketError> {
        self.channel_sender
            .try_send((*address, response))
            .map_err(|err| match err {
                TrySendError::Full(_) => unreachable!("the channel is expected to be unbound"),
                TrySendError::Closed(_) => NaiaServerSocketError::SendError(*address),
            })
    }
}
