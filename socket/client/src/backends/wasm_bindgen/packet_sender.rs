use js_sys::Uint8Array;
use web_sys::MessagePort;

use crate::{error::NaiaClientSocketError, server_addr::ServerAddr};

use super::{addr_cell::AddrCell, data_port::DataPort};

/// Handles sending messages to the Server for a given Client Socket
#[derive(Clone)]
pub struct PacketSender {
    message_port: MessagePort,
    server_addr: AddrCell,
    connected: bool,
}

impl PacketSender {
    /// Create a new PacketSender
    pub fn new(data_port: &DataPort, addr_cell: &AddrCell) -> Self {
        PacketSender {
            message_port: data_port.message_port(),
            server_addr: addr_cell.clone(),
            connected: true,
        }
    }
}

impl PacketSender {
    /// Send a Packet to the Server
    pub fn send(&self, payload: &[u8]) -> Result<(), NaiaClientSocketError> {
        if self.connected {
            let uarray: Uint8Array = payload.into();
            self.message_port
                .post_message(&uarray)
                .expect("Failed to send message");
            Ok(())
        } else {
            Err(NaiaClientSocketError::SendError)
        }
    }

    /// Get the Server's Socket address
    pub fn server_addr(&self) -> ServerAddr {
        self.server_addr.get()
    }

    pub fn connected(&self) -> bool {
        self.connected
    }

    pub fn disconnect(&mut self) {
        if self.connected {
            self.connected = false;
            self.message_port.close();
        }
    }
}

// Safety: wasm32-unknown-unknown is single-threaded; there are no real OS threads and no
// data races are possible. These impls are required by trait bounds in the naia transport
// abstraction layer but are vacuously safe on this target.

#[allow(dead_code)]
fn _assert_packet_sender_send_sync() {
    fn require<T: Send + Sync>() {}
    require::<PacketSender>();
}
