use super::shared::table_mut;
use crate::{socket_table::SocketId, IdentityReceiverResult};

/// Handles receiving an IdentityToken from the Server through a given Client Socket
#[derive(Clone)]
pub struct IdentityReceiver {
    socket_id: u32,
}

impl IdentityReceiver {
    /// Create a new IdentityReceiver bound to one socket's slot in the shared
    /// table. Only `Socket::connect_inner` calls this, with a freshly
    /// allocated id, so two sockets never share a cell (naia-lib/naia#193).
    pub(crate) fn new(socket_id: u32) -> Self {
        IdentityReceiver { socket_id }
    }

    pub fn receive(&mut self) -> IdentityReceiverResult {
        // A disconnected socket's slot reads as Waiting.
        if let Some(table) = table_mut() {
            if let Some(state) = table.get_mut(SocketId(self.socket_id)) {
                // A non-200 signaling answer arrives on the identity path, never
                // through the packet ERROR_QUEUE and never via a data channel a
                // rejected handshake will not create. Consume it exactly once as
                // an ErrorResponseCode; afterwards there is nothing more to report
                // until the next handshake, so fall back to Waiting.
                if let Some(auth_error_cell) = &mut state.auth_error_cell {
                    if let Some((status, body)) = auth_error_cell.take() {
                        return IdentityReceiverResult::ErrorResponseCode(
                            status,
                            decode_reject_payload(&body),
                        );
                    }
                }
                if let Some(id_cell) = &mut state.id_cell {
                    if let Some(id_token) = id_cell.take() {
                        return IdentityReceiverResult::Success(id_token);
                    }
                }
            }
        }

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
    use super::super::shared::{table_mut, SOCKET_TABLE};
    use super::{decode_reject_payload, IdentityReceiver};
    use crate::IdentityReceiverResult;

    /// A non-200 answer is consumed exactly once as an ErrorResponseCode with
    /// the exact decoded body, and then reports Waiting; generic packet errors
    /// stay on their own queue -- and a second socket's cells are untouched.
    #[test]
    fn non_200_answer_is_consumed_once_while_generic_errors_stay_queued() {
        use super::super::shared::alloc_socket;
        use crate::socket_table::SocketId;

        let expected: &[u8] = b"miniquad-reject-reason";
        let first = alloc_socket();
        let second = alloc_socket();

        if let Some(table) = table_mut() {
            if let Some(state) = table.get_mut(SocketId(second)) {
                state
                    .error_queue
                    .push_back("data channel error".to_string());
                if let Some(auth_error_cell) = &mut state.auth_error_cell {
                    *auth_error_cell = Some((409, base64::encode(expected)));
                }
            }
        }

        // A nonempty body must arrive as its exact decoded bytes, not None:
        // dropping it here would silently erase the server's reason.
        let mut receiver = IdentityReceiver::new(second);
        match receiver.receive() {
            IdentityReceiverResult::ErrorResponseCode(409, Some(bytes)) => {
                assert_eq!(
                    bytes, expected,
                    "the rejection body must decode to its exact bytes",
                );
            }
            _ => panic!("a 409 with a reason body must surface once with its exact decoded bytes"),
        }
        assert!(matches!(
            receiver.receive(),
            IdentityReceiverResult::Waiting
        ));

        // The generic error was never touched by the identity path.
        if let Some(table) = table_mut() {
            let state = table.get(SocketId(second)).expect("socket live");
            assert_eq!(state.error_queue.len(), 1);
            assert_eq!(state.error_queue[0], "data channel error");
        } else {
            panic!("table initialized");
        }

        // The other socket's handshake is still outstanding, not consumed.
        let mut other = IdentityReceiver::new(first);
        assert!(matches!(other.receive(), IdentityReceiverResult::Waiting));

        // `decode_reject_payload` itself: empty means no reason, garbage keeps
        // the rejection with no reason.
        assert_eq!(decode_reject_payload(""), None);
        assert_eq!(decode_reject_payload("!!!"), None);

        unsafe {
            SOCKET_TABLE = None;
        }
    }
}
