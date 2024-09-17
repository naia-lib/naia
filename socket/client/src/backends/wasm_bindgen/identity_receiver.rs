use std::sync::{Arc, Mutex};

use naia_socket_shared::IdentityToken;

use crate::{identity_receiver::IdentityReceiver, IdentityReceiverResult};

/// Handles receiving an IdentityToken from the Server through a given Client Socket
#[derive(Clone)]
pub struct IdentityReceiverImpl {
    id_cell: Arc<Mutex<Option<Result<String, u16>>>>,
}

impl IdentityReceiverImpl {
    /// Create a new IdentityReceiver, if supplied with the Server's address & a
    /// reference back to the parent Socket
    pub fn new() -> Self {
        Self {
            id_cell: Arc::new(Mutex::new(None)),
        }
    }

    // this is for the DataChannel to send the IdentityToken to be picked up by the IdentityReceiver
    pub fn send(&self, id_token: IdentityToken) {
        let mut token_guard = self
            .id_cell
            .lock()
            .expect("This should never happen, message_queue should always be available in a single-threaded context");

        *token_guard = Some(Ok(id_token));
    }
}

impl IdentityReceiver for IdentityReceiverImpl {
    fn receive(&mut self) -> IdentityReceiverResult {
        let mut token_guard = self
            .id_cell
            .lock()
            .expect("This should never happen, message_queue should always be available in a single-threaded context");

        if token_guard.is_some() {
            let token_result = token_guard.take().unwrap();
            match token_result {
                Ok(token) => return IdentityReceiverResult::Success(token),
                Err(error_code) => return IdentityReceiverResult::ErrorResponseCode(error_code),
            }
        } else {
            return IdentityReceiverResult::Waiting;
        }
    }
}
