use crate::{error::NaiaClientSocketError, server_addr::ServerAddr, socket_table::SocketId};

use super::shared::SOCKET_TABLE;

/// Handles receiving messages from the Server through a given Client Socket
#[derive(Clone)]
pub struct PlainPacketReceiver {
    socket_id: u32,
    last_payload: Option<Box<[u8]>>,
}

impl PlainPacketReceiver {
    /// Create a new PacketReceiver bound to one socket's slot in the shared
    /// table. Only `Socket::connect_inner` calls this, with a freshly
    /// allocated id, so two sockets never share queues (naia-lib/naia#193).
    pub fn new(socket_id: u32) -> Self {
        PlainPacketReceiver {
            socket_id,
            last_payload: None,
        }
    }
}

impl PlainPacketReceiver {
    pub fn receive(&mut self) -> Result<Option<&[u8]>, NaiaClientSocketError> {
        // Safety: SOCKET_TABLE is a static mut written by the JS bridge
        // callbacks before receive() is ever called. wasm32 is
        // single-threaded; the JS bridge and Rust game loop are on the same
        // thread and never run concurrently, so accessing it without
        // synchronization is safe on this target. A disconnected socket's
        // slot reads as no traffic.
        unsafe {
            if let Some(table) = &mut SOCKET_TABLE {
                if let Some(state) = table.get_mut(SocketId(self.socket_id)) {
                    if let Some(message) = state.message_queue.pop_front() {
                        self.last_payload = Some(message);
                        return Ok(Some(self.last_payload.as_ref().unwrap()));
                    }

                    if let Some(error) = state.error_queue.pop_front() {
                        return Err(NaiaClientSocketError::Message(error));
                    }
                }
            }
        };

        Ok(None)
    }

    /// Get the Server's Socket address
    pub fn server_addr(&self) -> ServerAddr {
        // Safety: SOCKET_TABLE is read here from the same wasm32 thread the
        // JS bridge callbacks write. A disconnected socket reads as Finding.
        unsafe {
            if let Some(table) = &mut SOCKET_TABLE {
                if let Some(state) = table.get(SocketId(self.socket_id)) {
                    return state.server_addr;
                }
            }
        }
        ServerAddr::Finding
    }
}
