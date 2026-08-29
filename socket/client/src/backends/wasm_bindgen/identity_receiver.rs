use std::sync::{Arc, Mutex};

use naia_socket_shared::IdentityToken;

use crate::IdentityReceiverResult;

/// The signaling result the session request produced: an identity token, or a
/// status code with the (base64) body that came with it.
type IdentityResult = Result<IdentityToken, (u16, String)>;

/// Handles receiving an IdentityToken from the Server through a given Client Socket
#[derive(Clone)]
pub struct IdentityReceiver {
    id_cell: Arc<Mutex<Option<IdentityResult>>>,
}

impl IdentityReceiver {
    /// Create a new IdentityReceiver, if supplied with the Server's address & a
    /// reference back to the parent Socket
    pub fn new() -> Self {
        Self {
            id_cell: Arc::new(Mutex::new(None)),
        }
    }

    // this is for the DataChannel to send the IdentityToken to be picked up by the IdentityReceiver
    pub fn send(&self, id_token: IdentityToken) {
        *self.lock() = Some(Ok(id_token));
    }

    /// Records a non-200 signaling response, along with its body -- which on a
    /// rejection carries the optional base64-encoded reason message
    /// (naia-lib/naia#133).
    pub fn send_error(&self, status: u16, body: String) {
        *self.lock() = Some(Err((status, body)));
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<IdentityResult>> {
        self.id_cell
            .lock()
            .expect("This should never happen, message_queue should always be available in a single-threaded context")
    }
}

impl Default for IdentityReceiver {
    fn default() -> Self {
        Self::new()
    }
}

impl IdentityReceiver {
    pub fn receive(&mut self) -> IdentityReceiverResult {
        let Some(result) = self.lock().take() else {
            return IdentityReceiverResult::Waiting;
        };
        match result {
            Ok(token) => IdentityReceiverResult::Success(token),
            Err((status, body)) => {
                IdentityReceiverResult::ErrorResponseCode(status, decode_reject_payload(&body))
            }
        }
    }
}

/// Decodes the base64 body of a rejection response into raw message bits.
///
/// An empty body means "no reason given". A non-empty body that does not decode
/// is a malformed rejection: drop the reason but keep the rejection, which is
/// the part the application must act on.
fn decode_reject_payload(body: &str) -> Option<Vec<u8>> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }
    base64::decode(trimmed).ok()
}
