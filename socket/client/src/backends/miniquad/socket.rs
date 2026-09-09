use std::collections::VecDeque;

use naia_socket_shared::{parse_server_url, SocketConfig};

use crate::packet_receiver::PacketReceiver;

use super::{
    identity_receiver::IdentityReceiver,
    packet_receiver::PlainPacketReceiver,
    packet_sender::PacketSender,
    shared::{naia_connect, JsObject, AUTH_ERROR_CELL, ERROR_QUEUE, ID_CELL, MESSAGE_QUEUE},
};

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
        let server_url = parse_server_url(server_session_url);

        let auth_str: String = match auth_bytes_opt {
            Some(auth_bytes) => base64::encode(auth_bytes),
            None => "".to_string(),
        };

        // Safety: connect() is called once at socket startup before any callbacks fire.
        // ID_CELL, AUTH_ERROR_CELL, MESSAGE_QUEUE, and ERROR_QUEUE are written here and
        // subsequently only accessed from the same wasm32 thread via the JS bridge
        // callbacks and receive().
        unsafe {
            ID_CELL = Some(None);
            AUTH_ERROR_CELL = Some(None);
            MESSAGE_QUEUE = Some(VecDeque::new());
            ERROR_QUEUE = Some(VecDeque::new());
            // The fingerprint is passed separately from `auth_str`, and the
            // JS bridge sets it as its own header after the Authorization one,
            // so it cannot be folded into or displaced by the credential.
            naia_connect(
                JsObject::string(server_url.to_string().as_str()),
                JsObject::string(config.rtc_endpoint_path.as_str()),
                JsObject::string(auth_str.as_str()),
                JsObject::string(protocol_id),
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

#[cfg(test)]
mod js_bridge_contract_tests {
    use naia_socket_shared::PROTOCOL_ID_HEADER;

    /// The JS half of this backend is the one consumer of the header name that
    /// cannot `use` the constant: it is a separate language. So the literal in
    /// `naia_socket.js` is a second copy by necessity, and this asserts the two
    /// still agree. If they drift, every miniquad client is refused by the
    /// session server, with a symptom (a 404 on the session POST) that points
    /// nowhere near the cause.
    const NAIA_SOCKET_JS: &str = include_str!("naia_socket.js");

    #[test]
    fn the_js_bridge_sets_the_same_header_name_this_crate_declares() {
        assert!(
            NAIA_SOCKET_JS.contains(&format!("setRequestHeader(\"{}\"", PROTOCOL_ID_HEADER)),
            "naia_socket.js must set the {PROTOCOL_ID_HEADER} header",
        );
    }

    /// The fingerprint has to reach the JS side to be set at all: the import
    /// shim, the `connect` entry point and the request all have to carry it.
    /// A bridge that accepted the argument and dropped it would set no header
    /// and fail closed at the server, which is safe but silent.
    #[test]
    fn the_js_bridge_threads_the_fingerprint_from_the_import_to_the_request() {
        assert!(
            NAIA_SOCKET_JS
                .contains("naia_connect = function (address, rtc_path, auth_str, protocol_id)"),
            "the imported shim must take the fingerprint",
        );
        assert!(
            NAIA_SOCKET_JS.contains(
                "connect: function (server_socket_address, rtc_path, auth_str, protocol_id)"
            ),
            "the connect entry point must take the fingerprint",
        );
        assert!(
            NAIA_SOCKET_JS.contains("naia_socket.get_js_object(protocol_id)"),
            "the fingerprint must be unwrapped from its JsObject handle",
        );
    }

    /// Naia's header is set last and unconditionally, after the consumer's
    /// `Authorization` header -- the same ordering `stamp_protocol_id_header`
    /// gives the other backends, and for the same reason: a caller must not be
    /// able to displace it, and a last-wins duplicate must resolve to naia's
    /// value.
    #[test]
    fn the_js_bridge_sets_the_fingerprint_after_the_credential() {
        let credential = NAIA_SOCKET_JS
            .find("setRequestHeader(\"Authorization\"")
            .expect("the bridge should still set Authorization");
        let fingerprint = NAIA_SOCKET_JS
            .find(&format!("setRequestHeader(\"{}\"", PROTOCOL_ID_HEADER))
            .expect("the bridge should set the fingerprint header");

        assert!(
            fingerprint > credential,
            "naia's header must be stamped after the caller's",
        );
    }
}
