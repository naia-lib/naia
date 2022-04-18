use crossbeam::channel::Sender;
use std::net::SocketAddr;

/// Used to send packets to the Server Socket
#[derive(Clone)]
pub struct PacketSender {
    channel_sender: Sender<(SocketAddr, Box<[u8]>)>,
}

impl PacketSender {
    /// Creates a new PacketSender
    pub fn new(channel_sender: Sender<(SocketAddr, Box<[u8]>)>) -> Self {
        PacketSender { channel_sender }
    }

    /// Sends a packet to the Server Socket
    pub fn send(&self, address: &SocketAddr, payload: &[u8]) {
        self.channel_sender
            .send((*address, payload.into()))
            .unwrap(); //TODO: handle result..
    }
}
