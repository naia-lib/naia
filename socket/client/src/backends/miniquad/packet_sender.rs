use crate::{error::NaiaClientSocketError, packet_sender::PacketSender, ServerAddr};

use super::shared::{naia_create_u8_array, naia_send, naia_disconnect, naia_is_connected, SERVER_ADDR};

/// Handles sending messages to the Server for a given Client Socket
#[derive(Clone, Default)]
pub struct PacketSenderImpl;

impl PacketSender for PacketSenderImpl {
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

    fn connected(&self) -> bool {
        unsafe {
            return naia_is_connected();
        }
    }

    fn disconnect(&mut self) {
        unsafe {
            naia_disconnect();
        }
    }
}
