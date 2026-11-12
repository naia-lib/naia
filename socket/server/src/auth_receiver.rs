use std::net::SocketAddr;

use smol::channel::Receiver;

use super::error::NaiaServerSocketError;

/// Used to receive Auth messages from the Server Socket
#[derive(Clone)]
pub struct AuthReceiver {
    #[allow(clippy::type_complexity)]
    channel_receiver: Receiver<Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError>>,
    last_payload: Option<Box<[u8]>>,
}

impl AuthReceiver {
    /// Creates a new `AuthReceiver`
    #[allow(clippy::type_complexity)]
    #[must_use]
    pub fn new(
        channel_receiver: Receiver<Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError>>,
    ) -> Self {
        Self {
            channel_receiver,
            last_payload: None,
        }
    }

    /// Receives an Auth message from the Server Socket
    // `Err` is currently unreachable, but the drain loop in `main_server`
    // branches on it so a persistently-failing auth channel cannot trap it;
    // keep the signature fallible.
    #[allow(clippy::unnecessary_wraps)]
    pub fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, NaiaServerSocketError> {
        match self.channel_receiver.try_recv() {
            Ok(result) => match result {
                Ok((address, payload)) => Ok(Some((address, self.last_payload.insert(payload)))),
                Err(_) => Ok(None),
            },
            Err(_) => Ok(None),
        }
    }
}
