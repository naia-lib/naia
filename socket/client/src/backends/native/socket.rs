use naia_socket_shared::{parse_server_url, SocketConfig};

use webrtc_unreliable_client::Socket as RTCSocket;

use super::{
    identity_receiver::IdentityReceiver, packet_receiver::PlainPacketReceiver,
    packet_sender::PacketSender,
};
use crate::{backends::native::runtime::get_runtime, packet_receiver::PacketReceiver};

/// A client-side socket which communicates with an underlying unordered &
/// unreliable protocol
pub struct Socket;

impl Socket {
    /// Connects to the given server address
    pub fn connect(
        server_session_url: &str,
        config: &SocketConfig,
    ) -> (IdentityReceiver, PacketSender, PacketReceiver) {
        Self::connect_inner(server_session_url, config, None, None)
    }

    /// Connects to the given server address with authentication
    pub fn connect_with_auth(
        server_session_url: &str,
        config: &SocketConfig,
        auth_bytes: Vec<u8>,
    ) -> (IdentityReceiver, PacketSender, PacketReceiver) {
        Self::connect_inner(server_session_url, config, Some(auth_bytes), None)
    }

    /// Connects to the given server address with authentication
    pub fn connect_with_auth_headers(
        server_session_url: &str,
        config: &SocketConfig,
        auth_headers: Vec<(String, String)>,
    ) -> (IdentityReceiver, PacketSender, PacketReceiver) {
        Self::connect_inner(server_session_url, config, None, Some(auth_headers))
    }

    /// Connects to the given server address with authentication
    pub fn connect_with_auth_and_headers(
        server_session_url: &str,
        config: &SocketConfig,
        auth_bytes: Vec<u8>,
        auth_headers: Vec<(String, String)>,
    ) -> (IdentityReceiver, PacketSender, PacketReceiver) {
        Self::connect_inner(
            server_session_url,
            config,
            Some(auth_bytes),
            Some(auth_headers),
        )
    }

    /// Connects to the given server address
    fn connect_inner(
        server_session_url: &str,
        config: &SocketConfig,
        auth_bytes_opt: Option<Vec<u8>>,
        auth_headers_opt: Option<Vec<(String, String)>>,
    ) -> (IdentityReceiver, PacketSender, PacketReceiver) {
        let server_session_string = format!(
            "{}{}",
            parse_server_url(server_session_url),
            config.rtc_endpoint_path.clone()
        );
        let conditioner_config = config.link_condition.clone();

        let (socket, io) = RTCSocket::new();
        get_runtime().spawn(async move {
            socket
                .connect(&server_session_string, auth_bytes_opt, auth_headers_opt)
                .await;
        });

        // Setup Packet Sender
        let packet_sender = PacketSender::new(
            io.addr_cell.clone(),
            io.to_server_sender,
            io.to_server_disconnect_sender,
        );

        // Setup Packet Receiver
        let inner_receiver = PlainPacketReceiver::new(io.addr_cell, io.to_client_receiver);
        let packet_receiver = PacketReceiver::new(inner_receiver, &conditioner_config);

        // Setup Identity Receiver
        let identity_receiver = IdentityReceiver::new(io.to_client_id_receiver);

        (identity_receiver, packet_sender, packet_receiver)
    }
}
