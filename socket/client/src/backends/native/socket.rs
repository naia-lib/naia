use naia_socket_shared::{parse_server_url, stamp_protocol_id_header, SocketConfig};

use webrtc_unreliable_client::Socket as RTCSocket;

use super::{
    addr_cell::AddrCell, identity_receiver::IdentityReceiver, packet_receiver::PlainPacketReceiver,
    packet_sender::PacketSender,
};
use crate::{backends::native::runtime::get_runtime, packet_receiver::PacketReceiver};

/// A client-side socket which communicates with an underlying unordered &
/// unreliable protocol
pub struct Socket;

impl Socket {
    /// Connects to the given server address
    /// `protocol_id` is this client's protocol fingerprint as 32 lowercase hex
    /// digits. Naia sends it as its own header on the session request; see
    /// [`stamp_protocol_id_header`](naia_socket_shared::stamp_protocol_id_header)
    /// for why it is stamped rather than left to the caller.
    pub fn connect(
        server_session_url: &str,
        config: &SocketConfig,
        protocol_id: &str,
    ) -> (IdentityReceiver, PacketSender, PacketReceiver) {
        Self::connect_inner(server_session_url, config, None, None, protocol_id)
    }

    /// Connects to the given server address with authentication
    pub fn connect_with_auth(
        server_session_url: &str,
        config: &SocketConfig,
        auth_bytes: Vec<u8>,
        protocol_id: &str,
    ) -> (IdentityReceiver, PacketSender, PacketReceiver) {
        Self::connect_inner(
            server_session_url,
            config,
            Some(auth_bytes),
            None,
            protocol_id,
        )
    }

    /// Connects to the given server address with authentication
    pub fn connect_with_auth_headers(
        server_session_url: &str,
        config: &SocketConfig,
        auth_headers: Vec<(String, String)>,
        protocol_id: &str,
    ) -> (IdentityReceiver, PacketSender, PacketReceiver) {
        Self::connect_inner(
            server_session_url,
            config,
            None,
            Some(auth_headers),
            protocol_id,
        )
    }

    /// Connects to the given server address with authentication
    pub fn connect_with_auth_and_headers(
        server_session_url: &str,
        config: &SocketConfig,
        auth_bytes: Vec<u8>,
        auth_headers: Vec<(String, String)>,
        protocol_id: &str,
    ) -> (IdentityReceiver, PacketSender, PacketReceiver) {
        Self::connect_inner(
            server_session_url,
            config,
            Some(auth_bytes),
            Some(auth_headers),
            protocol_id,
        )
    }

    /// Connects to the given server address
    fn connect_inner(
        server_session_url: &str,
        config: &SocketConfig,
        auth_bytes_opt: Option<Vec<u8>>,
        auth_headers_opt: Option<Vec<(String, String)>>,
        protocol_id: &str,
    ) -> (IdentityReceiver, PacketSender, PacketReceiver) {
        let server_session_string = format!(
            "{}{}",
            parse_server_url(server_session_url),
            config.rtc_endpoint_path.clone()
        );
        let conditioner_config = config.link_condition.clone();

        // Naia's own header, stamped after everything the caller supplied.
        let auth_headers_opt = Some(stamp_protocol_id_header(auth_headers_opt, protocol_id));

        let (socket, io) = RTCSocket::new();
        // The address arrives once, later, over a oneshot; AddrCell is the
        // polled view of it that the sender and receiver both hold.
        let addr_cell = AddrCell::new(io.to_client_addr_receiver);
        get_runtime().spawn(async move {
            socket
                .connect(&server_session_string, auth_bytes_opt, auth_headers_opt)
                .await;
        });

        // Setup Packet Sender
        let packet_sender = PacketSender::new(
            addr_cell.clone(),
            io.to_server_sender,
            io.to_server_disconnect_sender,
        );

        // Setup Packet Receiver
        let inner_receiver = PlainPacketReceiver::new(addr_cell, io.to_client_receiver);
        let packet_receiver = PacketReceiver::new(inner_receiver, &conditioner_config);

        // Setup Identity Receiver
        let identity_receiver = IdentityReceiver::new(io.to_client_id_receiver);

        (identity_receiver, packet_sender, packet_receiver)
    }
}
