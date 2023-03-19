use naia_socket_shared::SocketConfig;

use crate::{packet_sender::PacketSender, packet_receiver::PacketReceiver};

/// Used to send packets from the Client Socket
pub trait SocketTrait {
    fn connect(server_session_url: &str, config: &SocketConfig) -> (PacketSender, PacketReceiver);
}