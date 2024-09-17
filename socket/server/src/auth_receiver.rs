use std::net::SocketAddr;

use smol::channel::Receiver;

use super::error::NaiaServerSocketError;

/// Used to receive Auth messages from the Server Socket
pub trait AuthReceiver: AuthReceiverClone + Send + Sync {
    /// Receives an Auth message from the Server Socket
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, NaiaServerSocketError>;
}

/// Used to receive Auth messages from the Server Socket
#[derive(Clone)]
pub struct AuthReceiverImpl {
    #[allow(clippy::type_complexity)]
    channel_receiver: Receiver<Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError>>,
    last_payload: Option<Box<[u8]>>,
}

impl AuthReceiverImpl {
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
}

impl AuthReceiver for AuthReceiverImpl {
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, NaiaServerSocketError> {
        match self.channel_receiver.try_recv() {
            Ok(result) => match result {
                Ok((address, payload)) => {
                    self.last_payload = Some(payload);
                    return Ok(Some((address, self.last_payload.as_ref().unwrap())));
                }
                Err(_) => Ok(None),
            },
            Err(_) => Ok(None),
        }
    }
}

/// Used to clone Box<dyn AuthReceiver>
pub trait AuthReceiverClone {
    /// Clone the boxed AuthReceiver
    fn clone_box(&self) -> Box<dyn AuthReceiver>;
}

impl<T: 'static + AuthReceiver + Clone> AuthReceiverClone for T {
    fn clone_box(&self) -> Box<dyn AuthReceiver> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn AuthReceiver> {
    fn clone(&self) -> Box<dyn AuthReceiver> {
        AuthReceiverClone::clone_box(self.as_ref())
    }
}
