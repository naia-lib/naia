use naia_socket_shared::{stamp_protocol_id_header, SocketConfig};

use super::{
    addr_cell::AddrCell, data_channel::DataChannel, data_port::DataPort,
    identity_receiver::IdentityReceiver, packet_receiver::PlainPacketReceiver,
    packet_sender::PacketSender,
};
use crate::packet_receiver::PacketReceiver;

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
        return Self::connect_inner(server_session_url, config, None, None, protocol_id);
    }

    /// Connects to the given server address with authentication
    pub fn connect_with_auth(
        server_session_url: &str,
        config: &SocketConfig,
        auth_bytes: Vec<u8>,
        protocol_id: &str,
    ) -> (IdentityReceiver, PacketSender, PacketReceiver) {
        return Self::connect_inner(
            server_session_url,
            config,
            Some(auth_bytes),
            None,
            protocol_id,
        );
    }

    /// Connects to the given server address with authentication
    pub fn connect_with_auth_headers(
        server_session_url: &str,
        config: &SocketConfig,
        auth_headers: Vec<(String, String)>,
        protocol_id: &str,
    ) -> (IdentityReceiver, PacketSender, PacketReceiver) {
        return Self::connect_inner(
            server_session_url,
            config,
            None,
            Some(auth_headers),
            protocol_id,
        );
    }

    /// Connects to the given server address with authentication
    pub fn connect_with_auth_and_headers(
        server_session_url: &str,
        config: &SocketConfig,
        auth_bytes: Vec<u8>,
        auth_headers: Vec<(String, String)>,
        protocol_id: &str,
    ) -> (IdentityReceiver, PacketSender, PacketReceiver) {
        return Self::connect_inner(
            server_session_url,
            config,
            Some(auth_bytes),
            Some(auth_headers),
            protocol_id,
        );
    }

    /// Connects to the given server address
    fn connect_inner(
        server_session_url: &str,
        config: &SocketConfig,
        auth_bytes_opt: Option<Vec<u8>>,
        auth_headers_opt: Option<Vec<(String, String)>>,
        protocol_id: &str,
    ) -> (IdentityReceiver, PacketSender, PacketReceiver) {
        // Naia's own header, stamped after everything the caller supplied.
        let auth_headers = stamp_protocol_id_header(auth_headers_opt, protocol_id);

        let data_channel =
            DataChannel::new(config, server_session_url, auth_bytes_opt, auth_headers);

        let data_port = data_channel.data_port();
        let addr_cell = data_channel.addr_cell();

        let (packet_sender, packet_receiver) = Socket::setup_io(config, &addr_cell, &data_port);

        // Setup Identity Receiver
        let id_receiver: IdentityReceiver = data_channel.id_receiver();

        data_channel.start();

        return (id_receiver, packet_sender, packet_receiver);
    }

    // Creates a Socket from an underlying DataPort.
    // This is for use in apps running within a Web Worker.
    pub fn connect_with_data_port(
        config: &SocketConfig,
        data_port: &DataPort,
    ) -> (PacketSender, PacketReceiver) {
        let addr_cell = AddrCell::new();
        return Socket::setup_io(config, &addr_cell, data_port);
    }

    fn setup_io(
        config: &SocketConfig,
        addr_cell: &AddrCell,
        data_port: &DataPort,
    ) -> (PacketSender, PacketReceiver) {
        // Setup Packet Sender
        let packet_sender = PacketSender::new(&data_port, addr_cell);

        // Setup Packet Receiver
        let inner_receiver = PlainPacketReceiver::new(&data_port, addr_cell);
        let packet_receiver = PacketReceiver::new(inner_receiver, &config.link_condition);

        return (packet_sender, packet_receiver);
    }
}
