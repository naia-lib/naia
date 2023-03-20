use tokio::sync::mpsc::{error::SendError, UnboundedSender};
use webrtc_unreliable_client::{AddrCell, ServerAddr as RTCServerAddr};

use crate::{
    error::NaiaClientSocketError, packet_sender::PacketSender, server_addr::ServerAddr,
};

/// Handles sending messages to the Server for a given Client Socket
#[derive(Clone)]
pub struct PacketSenderImpl {
    server_addr: AddrCell,
    sender_channel: UnboundedSender<Box<[u8]>>,
}

impl PacketSenderImpl {
    /// Create a new PacketSender, if supplied with the Server's address & a
    /// reference back to the parent Socket
    pub fn new(server_addr: AddrCell, sender_channel: UnboundedSender<Box<[u8]>>) -> Self {
        PacketSenderImpl {
            server_addr,
            sender_channel,
        }
    }
}

impl PacketSender for PacketSenderImpl {
    /// Send a Packet to the Server
    fn send(&self, payload: &[u8]) -> Result<(), NaiaClientSocketError> {
        self.sender_channel
            .send(payload.into())
            .map_err(|_err: SendError<_>| NaiaClientSocketError::SendError)
    }

    /// Get the Server's Socket address
    fn server_addr(&self) -> ServerAddr {
        match self.server_addr.get() {
            RTCServerAddr::Finding => ServerAddr::Finding,
            RTCServerAddr::Found(addr) => ServerAddr::Found(addr),
        }
    }
}
