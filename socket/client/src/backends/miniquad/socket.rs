use std::collections::VecDeque;

use naia_socket_shared::{parse_server_url, SocketConfig};

use crate::{
    backends::socket::SocketTrait, conditioned_packet_receiver::ConditionedPacketReceiver,
    packet_receiver::PacketReceiver, packet_sender::PacketSender, IdentityReceiver,
    IdentityReceiverImpl,
};

use super::{
    packet_receiver::PacketReceiverImpl,
    packet_sender::PacketSenderImpl,
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
    ) -> (
        Box<dyn IdentityReceiver>,
        Box<dyn PacketSender>,
        Box<dyn PacketReceiver>,
    ) {
        return Self::connect_inner(server_session_url, config, None, None);
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
        return Self::connect_inner(server_session_url, config, Some(auth_bytes), None);
    }

    /// Connects to the given server address with authentication
    pub fn connect_with_auth_headers(
        server_session_url: &str,
        config: &SocketConfig,
        auth_headers: Vec<(String, String)>,
    ) -> (
        Box<dyn IdentityReceiver>,
        Box<dyn PacketSender>,
        Box<dyn PacketReceiver>,
    ) {
        return Self::connect_inner(server_session_url, config, None, Some(auth_headers));
    }

    /// Connects to the given server address with authentication
    pub fn connect_with_auth_and_headers(
        server_session_url: &str,
        config: &SocketConfig,
        auth_bytes: Vec<u8>,
        auth_headers: Vec<(String, String)>,
    ) -> (
        Box<dyn IdentityReceiver>,
        Box<dyn PacketSender>,
        Box<dyn PacketReceiver>,
    ) {
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
    ) -> (
        Box<dyn IdentityReceiver>,
        Box<dyn PacketSender>,
        Box<dyn PacketReceiver>,
    ) {
        let server_url = parse_server_url(server_session_url);

        let auth_str: String = match auth_bytes_opt {
            Some(auth_bytes) => base64::encode(auth_bytes),
            None => "".to_string(),
        };

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

        // setup id receiver
        let id_receiver: Box<dyn IdentityReceiver> = Box::new(IdentityReceiverImpl);

        return (id_receiver, packet_sender, packet_receiver);
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
