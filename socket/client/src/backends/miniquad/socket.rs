use std::collections::VecDeque;

use naia_socket_shared::{parse_server_url, SocketConfig};

use crate::{
    backends::socket::SocketTrait, conditioned_packet_receiver::ConditionedPacketReceiver,
    packet_receiver::PacketReceiver, packet_sender::PacketSender,
};

use super::{
    packet_receiver::PacketReceiverImpl,
    packet_sender::PacketSenderImpl,
    shared::{naia_connect, JsObject, ERROR_QUEUE, MESSAGE_QUEUE},
};

/// A client-side socket which communicates with an underlying unordered &
/// unreliable protocol
pub struct Socket;

impl Socket {
    /// Connects to the given server address
    pub fn connect(
        server_session_url: &str,
        config: &SocketConfig,
    ) -> (Box<dyn PacketSender>, Box<dyn PacketReceiver>) {
        let server_url = parse_server_url(server_session_url);

        unsafe {
            MESSAGE_QUEUE = Some(VecDeque::new());
            ERROR_QUEUE = Some(VecDeque::new());
            naia_connect(
                JsObject::string(server_url.to_string().as_str()),
                JsObject::string(config.rtc_endpoint_path.as_str()),
            );
        }

        let conditioner_config = config.link_condition.clone();

        // setup sender
        let packet_sender: Box<dyn PacketSender> = Box::new(PacketSenderImpl);

        // setup receiver
        let packet_receiver: Box<dyn PacketReceiver> = {
            let inner_receiver = Box::new(PacketReceiverImpl::new());
            if let Some(config) = &conditioner_config {
                Box::new(ConditionedPacketReceiver::new(inner_receiver, config))
            } else {
                inner_receiver
            }
        };

        return (packet_sender, packet_receiver);
    }
}

impl SocketTrait for Socket {
    /// Connects to the given server address
    fn connect(
        server_session_url: &str,
        config: &SocketConfig,
    ) -> (Box<dyn PacketSender>, Box<dyn PacketReceiver>) {
        return Socket::connect(server_session_url, config);
    }
}
