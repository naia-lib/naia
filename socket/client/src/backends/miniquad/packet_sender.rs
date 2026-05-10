use crate::{error::NaiaClientSocketError, packet_sender::PacketSender, ServerAddr};

use super::shared::{
    naia_create_u8_array, naia_disconnect, naia_is_connected, naia_send, SERVER_ADDR,
};

/// Handles sending messages to the Server for a given Client Socket
#[derive(Clone, Default)]
pub struct PacketSenderImpl;

impl PacketSender for PacketSenderImpl {
    /// Send a Packet to the Server
    fn send(&self, payload: &[u8]) -> Result<(), NaiaClientSocketError> {
        // Safety: naia_create_u8_array and naia_send are extern "C" FFI functions provided
        // by the miniquad JavaScript bridge. wasm32 is single-threaded; SERVER_ADDR and the
        // JS object handle are accessed without aliasing. The pointer passed to
        // naia_create_u8_array is valid for the duration of the call (payload is borrowed).
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
        // Safety: SERVER_ADDR is a static mut set once at socket initialization before any
        // PacketSender is cloned. wasm32 is single-threaded; there are no concurrent writes.
        unsafe { SERVER_ADDR }
    }

    fn connected(&self) -> bool {
        // Safety: naia_is_connected() is a read-only FFI call into the JS bridge; no preconditions.
        unsafe {
            return naia_is_connected();
        }
    }

    fn disconnect(&mut self) {
        // Safety: naia_disconnect() is an FFI call with no return value or preconditions.
        unsafe {
            naia_disconnect();
        }
    }
}
