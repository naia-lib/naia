use super::shared::ID_CELL;
use crate::{identity_receiver::IdentityReceiver, IdentityReceiverResult};

/// Handles receiving an IdentityToken from the Server through a given Client Socket
#[derive(Clone)]
pub struct IdentityReceiverImpl;

impl IdentityReceiver for IdentityReceiverImpl {
    fn receive(&mut self) -> IdentityReceiverResult {
        // Safety: ID_CELL is a static mut written by the JS identity callback.
        // wasm32 is single-threaded; no concurrent access is possible.
        unsafe {
            if let Some(id_cell) = &mut ID_CELL {
                if let Some(id_token) = id_cell.take() {
                    return IdentityReceiverResult::Success(id_token);
                }
            }
        };

        IdentityReceiverResult::Waiting
    }
}
