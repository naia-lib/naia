use super::shared::{AUTH_ERROR_CELL, ID_CELL};
use crate::IdentityReceiverResult;

/// Handles receiving an IdentityToken from the Server through a given Client Socket
#[derive(Clone)]
pub struct IdentityReceiver;

impl IdentityReceiver {
    pub fn receive(&mut self) -> IdentityReceiverResult {
        // Safety: AUTH_ERROR_CELL and ID_CELL are static muts written by the
        // JS bridge callbacks. wasm32 is single-threaded; no concurrent access
        // is possible.
        unsafe {
            // A non-200 signaling answer arrives on the identity path, never
            // through the packet ERROR_QUEUE and never via a data channel a
            // rejected handshake will not create. Consume it exactly once as
            // an ErrorResponseCode; afterwards there is nothing more to report
            // until the next handshake, so fall back to Waiting.
            if let Some(auth_error_cell) = &mut AUTH_ERROR_CELL {
                if let Some((status, body)) = auth_error_cell.take() {
                    return IdentityReceiverResult::ErrorResponseCode(
                        status,
                        decode_reject_payload(&body),
                    );
                }
            }
            if let Some(id_cell) = &mut ID_CELL {
                if let Some(id_token) = id_cell.take() {
                    return IdentityReceiverResult::Success(id_token);
                }
            }
        };

        IdentityReceiverResult::Waiting
    }
}

/// Decodes the base64 body of a rejection response into raw message bits.
///
/// An empty body means "no reason given". A non-empty body that does not
/// decode is a malformed rejection: drop the reason but keep the rejection,
/// which is the part the application must act on. Mirrors the wasm-bindgen
/// backend's helper; the two backends cannot share it without a new shared
/// module, and the function is three lines.
fn decode_reject_payload(body: &str) -> Option<Vec<u8>> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }
    base64::decode(trimmed).ok()
}

#[cfg(test)]
mod auth_error_cell_tests {
    use super::super::shared::{AUTH_ERROR_CELL, ERROR_QUEUE};
    use super::IdentityReceiver;
    use crate::IdentityReceiverResult;

    use std::collections::VecDeque;

    /// A non-200 answer is consumed exactly once as an ErrorResponseCode and
    /// then reports Waiting; generic packet errors stay on their own queue.
    #[test]
    fn non_200_answer_is_consumed_once_while_generic_errors_stay_queued() {
        unsafe {
            AUTH_ERROR_CELL = Some(None);
            ERROR_QUEUE = Some(VecDeque::new());
        }

        // A generic packet error is already waiting on its own queue.
        unsafe {
            if let Some(error_queue) = &mut ERROR_QUEUE {
                error_queue.push_back("data channel error".to_string());
            }
            if let Some(auth_error_cell) = &mut AUTH_ERROR_CELL {
                *auth_error_cell = Some((409, String::new()));
            }
        }

        let mut receiver = IdentityReceiver;
        assert!(matches!(
            receiver.receive(),
            IdentityReceiverResult::ErrorResponseCode(409, None)
        ));
        assert!(matches!(
            receiver.receive(),
            IdentityReceiverResult::Waiting
        ));

        // The generic error was never touched by the identity path.
        unsafe {
            let error_queue = ERROR_QUEUE.as_ref().expect("queue initialized");
            assert_eq!(error_queue.len(), 1);
            assert_eq!(error_queue[0], "data channel error");
        }

        unsafe {
            AUTH_ERROR_CELL = None;
            ERROR_QUEUE = None;
        }
    }
}
