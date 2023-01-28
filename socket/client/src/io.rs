use crate::{PacketReceiver, PacketSender};

/// Contains internal socket packet sender/receiver
pub(crate) struct Io {
    /// Used to send packets through the socket
    pub packet_sender: PacketSender,
    /// Used to receive packets from the socket
    pub packet_receiver: Option<PacketReceiver>,
}

impl Io {
    pub fn new(packet_sender: PacketSender, packet_receiver: PacketReceiver) -> Self {
        Self {
            packet_sender,
            packet_receiver: Some(packet_receiver),
        }
    }
}
