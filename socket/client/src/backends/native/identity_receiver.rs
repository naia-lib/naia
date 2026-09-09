use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

use naia_socket_shared::IdentityToken;

use webrtc_unreliable_client::SessionError;

use crate::IdentityReceiverResult;

/// Handles receiving an IdentityToken from the Server through a given Client Socket
#[derive(Clone)]
pub struct IdentityReceiver {
    receiver_channel: Arc<Mutex<oneshot::Receiver<Result<String, SessionError>>>>,
}

impl IdentityReceiver {
    /// Create a new IdentityReceiver, if supplied with the Server's address & a
    /// reference back to the parent Socket
    pub fn new(receiver_channel: oneshot::Receiver<Result<String, SessionError>>) -> Self {
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
                            None => IdentityReceiverResult::ErrorResponseCode(400, None),
                        }
                    }
                    // A rejection may carry a base64-encoded message saying
                    // why (naia-lib/naia#133).
                    Err(error) => IdentityReceiverResult::ErrorResponseCode(
                        error.status_code,
                        decode_reject_payload(&error.body),
                    ),
                }
            } else {
                IdentityReceiverResult::Waiting
            }
        } else {
            IdentityReceiverResult::Waiting
        }
    }
}

/// Decodes the base64 body of a rejection response into the message bytes the
/// client's protocol will read. A body that isn't valid base64 can't be one of
/// ours, so it counts as no reason given.
fn decode_reject_payload(body: &str) -> Option<Vec<u8>> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }
    match base64::decode(trimmed) {
        Ok(bytes) => Some(bytes),
        Err(e) => {
            log::warn!("Rejection response carried an undecodable body: {:?}", e);
            None
        }
    }
}

#[cfg(test)]
mod session_error_tests {
    use super::IdentityReceiver;
    use crate::IdentityReceiverResult;
    use webrtc_unreliable_client::SessionError;

    /// A pre-auth signaling failure (e.g. 409 with ICE/data still unavailable)
    /// surfaces exactly once as an ErrorResponseCode with the exact status,
    /// then reports Waiting. The data-address question is settled one layer
    /// up, where Finding honestly becomes None.
    #[test]
    fn pre_auth_session_error_is_surfaced_once_then_waiting() {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let mut receiver = IdentityReceiver::new(receiver);
        sender
            .send(Err(SessionError {
                status_code: 409,
                body: String::new(),
            }))
            .expect("receiver alive");

        assert!(matches!(
            receiver.receive(),
            IdentityReceiverResult::ErrorResponseCode(409, None)
        ));
        assert!(matches!(
            receiver.receive(),
            IdentityReceiverResult::Waiting
        ));
    }
}
