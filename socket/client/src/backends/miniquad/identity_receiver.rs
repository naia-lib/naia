use naia_socket_shared::IdentityToken;

use crate::{
    error::NaiaClientSocketError, identity_receiver::IdentityReceiver,
};
use super::shared::ID_CELL;

/// Handles receiving an IdentityToken from the Server through a given Client Socket
#[derive(Clone)]
pub struct IdentityReceiverImpl;

impl IdentityReceiver for IdentityReceiverImpl {
    fn receive(&mut self) -> Result<Option<IdentityToken>, NaiaClientSocketError> {
        unsafe {
            if let Some(id_cell) = &mut ID_CELL {
                if let Some(id_token) = id_cell.take() {
                    return Ok(Some(id_token));
                }
            }
        };

        Ok(None)
    }
}
