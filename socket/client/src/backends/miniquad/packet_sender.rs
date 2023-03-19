use crate::{error::NaiaClientSocketError, packet_sender::PacketSenderTrait, ServerAddr};

use super::shared::{naia_create_u8_array, naia_send, SERVER_ADDR};

/// Handles sending messages to the Server for a given Client Socket
#[derive(Clone, Default)]
pub struct PacketSenderImpl;

impl PacketSenderTrait for PacketSenderImpl {
    /// Send a Packet to the Server
    fn send(&self, payload: &[u8]) -> Result<(), NaiaClientSocketError> {
        unsafe {
            let ptr = payload.as_ptr();
            let len = payload.len();
            let js_obj = naia_create_u8_array(ptr as _, len as _);
            return if naia_send(js_obj) {
                Ok(())
            } else {
                Err(NaiaClientSocketError::SendError)
            };
        }
    }

    /// Get the Server's Socket address
    fn server_addr(&self) -> ServerAddr {
        unsafe { SERVER_ADDR }
    }
}
