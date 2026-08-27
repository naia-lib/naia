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
    /// Creates a new AuthReceiver
    #[allow(clippy::type_complexity)]
    pub fn new(
        channel_receiver: Receiver<Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError>>,
    ) -> Self {
        Self {
            channel_receiver,
            last_payload: None,
        }
    }

    /// Receives an Auth message from the Server Socket
    pub fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, NaiaServerSocketError> {
        match self.channel_receiver.try_recv() {
            Ok(result) => match result {
                Ok((address, payload)) => {
                    self.last_payload = Some(payload);
                    Ok(Some((address, self.last_payload.as_ref().unwrap())))
                }
                Err(_) => Ok(None),
            },
            Err(_) => Ok(None),
        }
    }
}
