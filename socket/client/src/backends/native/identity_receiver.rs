use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

use naia_socket_shared::IdentityToken;

use crate::{error::NaiaClientSocketError, identity_receiver::IdentityReceiver};

/// Handles receiving an IdentityToken from the Server through a given Client Socket
#[derive(Clone)]
pub struct IdentityReceiverImpl {
    receiver_channel: Arc<Mutex<oneshot::Receiver<IdentityToken>>>,
}

impl IdentityReceiverImpl {
    /// Create a new IdentityReceiver, if supplied with the Server's address & a
    /// reference back to the parent Socket
    pub fn new(receiver_channel: oneshot::Receiver<IdentityToken>) -> Self {
        Self {
            receiver_channel: Arc::new(Mutex::new(receiver_channel)),
        }
    }
}

impl IdentityReceiver for IdentityReceiverImpl {
    fn receive(&mut self) -> Result<Option<IdentityToken>, NaiaClientSocketError> {
        // info!("IdentityReceiverImpl::receive - Called");
        if let Ok(mut receiver) = self.receiver_channel.lock() {
            // info!("IdentityReceiverImpl::receive - Lock acquired");
            if let Ok(token) = receiver.try_recv() {
                // info!("IdentityReceiverImpl::receive - Received IdentityToken");
                return Ok(Some(token));
            } else {
                // info!("IdentityReceiverImpl::receive - No IdentityToken available");
                return Ok(None);
            }
        } else {
            // info!("IdentityReceiverImpl::receive - Lock not acquired");
            return Ok(None);
        }
    }
}
