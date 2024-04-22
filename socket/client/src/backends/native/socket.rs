use naia_socket_shared::{parse_server_url, SocketConfig};

use webrtc_unreliable_client::Socket as RTCSocket;

use crate::{
    identity_receiver::IdentityReceiver,
    backends::{native::runtime::get_runtime, socket::SocketTrait},
    conditioned_packet_receiver::ConditionedPacketReceiver,
    packet_receiver::PacketReceiver,
    packet_sender::PacketSender,
    IdentityReceiverImpl,
};
use super::{packet_receiver::PacketReceiverImpl, packet_sender::PacketSenderImpl};

/// A client-side socket which communicates with an underlying unordered &
/// unreliable protocol
pub struct Socket;

impl Socket {
    /// Connects to the given server address
    pub fn connect(
        server_session_url: &str,
        config: &SocketConfig,
    ) -> (
        Box<dyn IdentityReceiver>,
        Box<dyn PacketSender>,
        Box<dyn PacketReceiver>,
    ) {
        return Self::connect_inner(server_session_url, config, None);
    }
    /// Connects to the given server address with authentication
    pub fn connect_with_auth(
        server_session_url: &str,
        config: &SocketConfig,
        auth_bytes: Vec<u8>,
    ) -> (
        Box<dyn IdentityReceiver>,
        Box<dyn PacketSender>,
        Box<dyn PacketReceiver>,
    ) {
        return Self::connect_inner(server_session_url, config, Some(auth_bytes));
    }
    /// Connects to the given server address
    fn connect_inner(
        server_session_url: &str,
        config: &SocketConfig,
        auth_bytes_opt: Option<Vec<u8>>,
    ) -> (
        Box<dyn IdentityReceiver>,
        Box<dyn PacketSender>,
        Box<dyn PacketReceiver>,
    ) {
        let server_session_string = format!(
            "{}{}",
            parse_server_url(server_session_url),
            config.rtc_endpoint_path.clone()
        );
        let conditioner_config = config.link_condition.clone();

        let (socket, io) = RTCSocket::new();
        get_runtime()
            .spawn(async move { socket.connect(&server_session_string, auth_bytes_opt).await });

        // Setup Packet Sender
        let packet_sender_impl = PacketSenderImpl::new(
            io.addr_cell.clone(),
            io.to_server_sender,
            io.to_server_disconnect_sender,
        );
        let packet_sender: Box<dyn PacketSender> = Box::new(packet_sender_impl);

        // Setup Packet Receiver
        let packet_receiver_impl = PacketReceiverImpl::new(io.addr_cell, io.to_client_receiver);
        let packet_receiver: Box<dyn PacketReceiver> = {
            let inner_receiver = Box::new(packet_receiver_impl);
            if let Some(config) = &conditioner_config {
                Box::new(ConditionedPacketReceiver::new(inner_receiver, config))
            } else {
                inner_receiver
            }
        };

        // Setup Identity Receiver
        let identity_receiver_impl = IdentityReceiverImpl::new(io.to_client_id_receiver);
        let identity_receiver: Box<dyn IdentityReceiver> = Box::new(identity_receiver_impl);

        return (identity_receiver, packet_sender, packet_receiver);
    }
}

impl SocketTrait for Socket {
    /// Connects to the given server address
    fn connect(
        server_session_url: &str,
        config: &SocketConfig,
    ) -> (
        Box<dyn IdentityReceiver>,
        Box<dyn PacketSender>,
        Box<dyn PacketReceiver>,
    ) {
        return Self::connect(server_session_url, config);
    }
    /// Connects to the given server address with authentication
    fn connect_with_auth(
        server_session_url: &str,
        config: &SocketConfig,
        auth_bytes: Vec<u8>,
    ) -> (
        Box<dyn IdentityReceiver>,
        Box<dyn PacketSender>,
        Box<dyn PacketReceiver>,
    ) {
        return Self::connect_with_auth(server_session_url, config, auth_bytes);
    }
}
