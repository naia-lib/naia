use js_sys::Uint8Array;
use web_sys::MessagePort;

use crate::{error::NaiaClientSocketError, server_addr::ServerAddr};

use super::{addr_cell::AddrCell, data_channel::WasmPeerCloser, data_port::DataPort};

/// Handles sending messages to the Server for a given Client Socket
#[derive(Clone)]
pub struct PacketSender {
    message_port: MessagePort,
    server_addr: AddrCell,
    connected: bool,
    /// The live peer/channel of this attempt. Retained here (not in the
    /// dropped DataChannel) so shutdown can close both on retry.
    closer: Option<WasmPeerCloser>,
}

impl PacketSender {
    /// Create a new PacketSender. `closer` is `Some` on the standard path
    /// (the peer/channel `start` built) and `None` for a worker-supplied
    /// `DataPort`, whose connection the host page owns.
    pub fn new(
        data_port: &DataPort,
        addr_cell: &AddrCell,
        closer: Option<WasmPeerCloser>,
    ) -> Self {
        PacketSender {
            message_port: data_port.message_port(),
            server_addr: addr_cell.clone(),
            connected: true,
            closer,
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
        self.shutdown();
    }

    /// Tears down this attempt's connection: closes the data channel, then
    /// the peer, then the message port, and refuses further sends. A stale
    /// peer left open would keep gathering and POSTing session offers while
    /// a retried attempt dials. Idempotent: the closer is taken on first
    /// call, so repeats only re-check the flag.
    pub fn shutdown(&mut self) {
        if self.connected {
            self.connected = false;
            if let Some(closer) = self.closer.take() {
                closer.close();
            }
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
