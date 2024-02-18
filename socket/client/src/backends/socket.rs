use naia_socket_shared::SocketConfig;

use crate::{packet_receiver::PacketReceiver, packet_sender::PacketSender};

/// Used to send packets from the Client Socket
#[allow(dead_code)]
pub trait SocketTrait {
    fn connect(
        server_session_url: &str,
        config: &SocketConfig,
    ) -> (Box<dyn PacketSender>, Box<dyn PacketReceiver>);
}
