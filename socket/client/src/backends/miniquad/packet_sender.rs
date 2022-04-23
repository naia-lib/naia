use super::shared::{naia_create_u8_array, naia_send, SERVER_ADDR};
use crate::ServerAddr;

/// Handles sending messages to the Server for a given Client Socket
#[derive(Clone, Default)]
pub struct PacketSender;

impl PacketSender {
    /// Send a Packet to the Server
    pub fn send(&self, payload: &[u8]) {
        unsafe {
            let ptr = payload.as_ptr();
            let len = payload.len();
            let js_obj = naia_create_u8_array(ptr as _, len as _);
            naia_send(js_obj);
        }
    }

    /// Get the Server's Socket address
    pub fn server_addr(&self) -> ServerAddr {
        unsafe { SERVER_ADDR }
    }
}
