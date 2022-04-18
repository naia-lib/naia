use super::{packet_receiver::PacketReceiver, packet_sender::PacketSender};

/// Contains internal socket packet sender/receiver
pub(crate) struct Io {
    /// Used to send packets through the socket
    pub packet_sender: PacketSender,
    /// Used to receive packets from the socket
    pub packet_receiver: PacketReceiver,
}
