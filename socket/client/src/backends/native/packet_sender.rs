use tokio::sync::mpsc::{error::SendError, UnboundedSender};
use webrtc_unreliable_client::{AddrCell, ServerAddr as RTCServerAddr};

use naia_socket_shared::ChannelClosedError;

use crate::server_addr::ServerAddr;

/// Handles sending messages to the Server for a given Client Socket
#[derive(Clone)]
pub struct PacketSender {
    server_addr: AddrCell,
    sender_channel: UnboundedSender<Box<[u8]>>,
}

impl PacketSender {
    /// Create a new PacketSender, if supplied with the Server's address & a
    /// reference back to the parent Socket
    pub fn new(server_addr: AddrCell, sender_channel: UnboundedSender<Box<[u8]>>) -> Self {
        PacketSender {
            server_addr,
            sender_channel,
        }
    }

    /// Send a Packet to the Server
    pub fn send(&self, payload: &[u8]) -> Result<(), ChannelClosedError<()>> {
        self.sender_channel
            .send(payload.into())
            .map_err(|_err: SendError<_>| ChannelClosedError(()))
    }

    /// Get the Server's Socket address
    pub fn server_addr(&self) -> ServerAddr {
        match self.server_addr.get() {
            RTCServerAddr::Finding => ServerAddr::Finding,
            RTCServerAddr::Found(addr) => ServerAddr::Found(addr),
        }
    }
}
