use tokio::sync::mpsc::Sender;
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
    pub fn send(&self, payload: &[u8]) {
        let _result = self.sender_channel.blocking_send(payload.into());
        // TODO: handle result
    }

    /// Get the Server's Socket address
    pub fn server_addr(&self) -> ServerAddr {
        match self.server_addr.get() {
            RTCServerAddr::Finding => ServerAddr::Finding,
            RTCServerAddr::Found(addr) => ServerAddr::Found(addr),
        }
    }
}
