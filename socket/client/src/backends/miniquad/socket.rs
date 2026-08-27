use std::collections::VecDeque;

use naia_socket_shared::{parse_server_url, SocketConfig};

use crate::packet_receiver::PacketReceiver;

use super::{
    identity_receiver::IdentityReceiver,
    packet_receiver::PlainPacketReceiver,
    packet_sender::PacketSender,
    shared::{naia_connect, JsObject, ERROR_QUEUE, ID_CELL, MESSAGE_QUEUE},
};

/// A client-side socket which communicates with an underlying unordered &
/// unreliable protocol
pub struct Socket;

impl Socket {
    /// Connects to the given server address
    pub fn connect(
        server_session_url: &str,
        config: &SocketConfig,
    ) -> (IdentityReceiver, PacketSender, PacketReceiver) {
        return Self::connect_inner(server_session_url, config, None, None);
    }

    /// Connects to the given server address with authentication
    pub fn connect_with_auth(
        server_session_url: &str,
        config: &SocketConfig,
        auth_bytes: Vec<u8>,
    ) -> (IdentityReceiver, PacketSender, PacketReceiver) {
        return Self::connect_inner(server_session_url, config, Some(auth_bytes), None);
    }

    /// Connects to the given server address with authentication
    pub fn connect_with_auth_headers(
        server_session_url: &str,
        config: &SocketConfig,
        auth_headers: Vec<(String, String)>,
    ) -> (IdentityReceiver, PacketSender, PacketReceiver) {
        return Self::connect_inner(server_session_url, config, None, Some(auth_headers));
    }

    /// Connects to the given server address with authentication
    pub fn connect_with_auth_and_headers(
        server_session_url: &str,
        config: &SocketConfig,
        auth_bytes: Vec<u8>,
        auth_headers: Vec<(String, String)>,
    ) -> (IdentityReceiver, PacketSender, PacketReceiver) {
        return Self::connect_inner(
            server_session_url,
            config,
            Some(auth_bytes),
            Some(auth_headers),
        );
    }

    /// Connects to the given server address
    fn connect_inner(
        server_session_url: &str,
        config: &SocketConfig,
        auth_bytes_opt: Option<Vec<u8>>,
        auth_headers_opt: Option<Vec<(String, String)>>,
    ) -> (IdentityReceiver, PacketSender, PacketReceiver) {
        let server_url = parse_server_url(server_session_url);

        let auth_str: String = match auth_bytes_opt {
            Some(auth_bytes) => base64::encode(auth_bytes),
            None => "".to_string(),
        };

        // Safety: connect() is called once at socket startup before any callbacks fire.
        // ID_CELL, MESSAGE_QUEUE, and ERROR_QUEUE are written here and subsequently only
        // accessed from the same wasm32 thread via the JS bridge callbacks and receive().
        unsafe {
            ID_CELL = Some(None);
            MESSAGE_QUEUE = Some(VecDeque::new());
            ERROR_QUEUE = Some(VecDeque::new());
            naia_connect(
                JsObject::string(server_url.to_string().as_str()),
                JsObject::string(config.rtc_endpoint_path.as_str()),
                JsObject::string(auth_str.as_str()),
            );
        }

        let conditioner_config = config.link_condition.clone();

        // setup sender
        let packet_sender = PacketSender;

        // setup receiver
        let inner_receiver = PlainPacketReceiver::new();
        let packet_receiver = PacketReceiver::new(inner_receiver, &conditioner_config);

        // setup id receiver
        let id_receiver = IdentityReceiver;

        return (id_receiver, packet_sender, packet_receiver);
    }
}
