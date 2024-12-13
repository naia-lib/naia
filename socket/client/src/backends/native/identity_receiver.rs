use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

use crate::{identity_receiver::IdentityReceiver, IdentityReceiverResult};

/// Handles receiving an IdentityToken from the Server through a given Client Socket
#[derive(Clone)]
pub struct IdentityReceiverImpl {
    receiver_channel: Arc<Mutex<oneshot::Receiver<Result<String, u16>>>>,
}

impl IdentityReceiverImpl {
    /// Create a new IdentityReceiver, if supplied with the Server's address & a
    /// reference back to the parent Socket
    pub fn new(receiver_channel: oneshot::Receiver<Result<String, u16>>) -> Self {
        Self {
            receiver_channel: Arc::new(Mutex::new(receiver_channel)),
        }
    }
}

impl IdentityReceiver for IdentityReceiverImpl {
    fn receive(&mut self) -> IdentityReceiverResult {
        if let Ok(mut receiver) = self.receiver_channel.lock() {
            if let Ok(recv_result) = receiver.try_recv() {
                match recv_result {
                    Ok(identity_token) => IdentityReceiverResult::Success(identity_token),
                    Err(error_code) => IdentityReceiverResult::ErrorResponseCode(error_code),
                }
            } else {
                IdentityReceiverResult::Waiting
            }
        } else {
            IdentityReceiverResult::Waiting
        }
    }
}
