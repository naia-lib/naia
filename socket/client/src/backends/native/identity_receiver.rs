use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

use naia_socket_shared::IdentityToken;

use crate::IdentityReceiverResult;

/// Handles receiving an IdentityToken from the Server through a given Client Socket
#[derive(Clone)]
pub struct IdentityReceiver {
    receiver_channel: Arc<Mutex<oneshot::Receiver<Result<String, u16>>>>,
}

impl IdentityReceiver {
    /// Create a new IdentityReceiver, if supplied with the Server's address & a
    /// reference back to the parent Socket
    pub fn new(receiver_channel: oneshot::Receiver<Result<String, u16>>) -> Self {
        Self {
            receiver_channel: Arc::new(Mutex::new(receiver_channel)),
        }
    }
}

impl IdentityReceiver {
    pub fn receive(&mut self) -> IdentityReceiverResult {
        if let Ok(mut receiver) = self.receiver_channel.lock() {
            if let Ok(recv_result) = receiver.try_recv() {
                match recv_result {
                    Ok(identity_token_string) => {
                        // webrtc-unreliable-client hands the signaling JSON's `id` field
                        // across verbatim as an unvalidated String.
                        match IdentityToken::from_signaling_string(&identity_token_string) {
                            Some(identity_token) => IdentityReceiverResult::Success(identity_token),
                            None => IdentityReceiverResult::ErrorResponseCode(400),
                        }
                    }
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
