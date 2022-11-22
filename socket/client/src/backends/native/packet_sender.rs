use tokio::sync::mpsc::{error::TrySendError, Sender};
use webrtc_unreliable_client::{AddrCell, ServerAddr as RTCServerAddr};

use crate::server_addr::ServerAddr;

/// Handles sending messages to the Server for a given Client Socket
#[derive(Clone)]
pub struct PacketSender {
    server_addr: AddrCell,
    sender_channel: Sender<Box<[u8]>>,
}

impl PacketSender {
    /// Create a new PacketSender, if supplied with the Server's address & a
    /// reference back to the parent Socket
    pub fn new(server_addr: AddrCell, sender_channel: Sender<Box<[u8]>>) -> Self {
        PacketSender {
            server_addr,
            sender_channel,
        }
    }

    /// Send a Packet to the Server
    pub fn send(&self, payload: &[u8]) -> Result<(), naia_socket_shared::TrySendError<()>> {
        self.sender_channel
            .try_send(payload.into())
            .map_err(|err| match err {
                TrySendError::Full(_) => naia_socket_shared::TrySendError::Full(()),
                TrySendError::Closed(_) => naia_socket_shared::TrySendError::Closed(()),
            })
    }

    /// Get the Server's Socket address
    pub fn server_addr(&self) -> ServerAddr {
        match self.server_addr.get() {
            RTCServerAddr::Finding => ServerAddr::Finding,
            RTCServerAddr::Found(addr) => ServerAddr::Found(addr),
        }
    }
}
