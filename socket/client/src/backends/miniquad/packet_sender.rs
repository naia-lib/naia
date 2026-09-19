use crate::{error::NaiaClientSocketError, socket_table::SocketId, ServerAddr};

use super::shared::{
    free_socket, naia_create_u8_array, naia_disconnect, naia_is_connected, naia_send, SOCKET_TABLE,
};

/// Handles sending messages to the Server for a given Client Socket
#[derive(Clone, Default)]
pub struct PacketSender {
    socket_id: u32,
}

impl PacketSender {
    /// Create a new PacketSender bound to one socket's slot in the shared
    /// table. Only `Socket::connect_inner` calls this, with a freshly
    /// allocated id, so two sockets never share a server address
    /// (naia-lib/naia#193).
    pub(crate) fn new(socket_id: u32) -> Self {
        PacketSender { socket_id }
    }

    /// Send a Packet to the Server
    pub fn send(&self, payload: &[u8]) -> Result<(), NaiaClientSocketError> {
        // Safety: naia_create_u8_array and naia_send are extern "C" FFI functions provided
        // by the miniquad JavaScript bridge. wasm32 is single-threaded; the
        // JS object handle is accessed without aliasing. The pointer passed to
        // naia_create_u8_array is valid for the duration of the call (payload is borrowed).
        // The socket id routes the send to this socket's connection.
        unsafe {
            let ptr = payload.as_ptr();
            let len = payload.len();
            let js_obj = naia_create_u8_array(ptr as _, len as _);
            return if naia_send(self.socket_id, js_obj) {
                Ok(())
            } else {
                Err(NaiaClientSocketError::SendError)
            };
        }
    }

    /// Get the Server's Socket address
    pub fn server_addr(&self) -> ServerAddr {
        // Safety: SOCKET_TABLE is a static mut read here from the same
        // wasm32 thread that the JS bridge callbacks write. A disconnected
        // socket's slot reads as still finding.
        unsafe {
            if let Some(table) = &mut SOCKET_TABLE {
                if let Some(state) = table.get(SocketId(self.socket_id)) {
                    return state.server_addr;
                }
            }
        }
        ServerAddr::Finding
    }

    pub fn connected(&self) -> bool {
        // Safety: naia_is_connected() is a read-only FFI call into the JS bridge; no preconditions.
        unsafe {
            return naia_is_connected(self.socket_id);
        }
    }

    pub fn disconnect(&mut self) {
        // Safety: naia_disconnect() is an FFI call with no return value or preconditions.
        // Freeing the slot drops this socket's queued state; other sockets'
        // slots are untouched.
        unsafe {
            naia_disconnect(self.socket_id);
        }
        free_socket(self.socket_id);
    }
}
