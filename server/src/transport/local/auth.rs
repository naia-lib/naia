use parking_lot::Mutex;
use std::{net::SocketAddr, sync::Arc};

use naia_shared::{IdentityToken, ProtocolId, PROTOCOL_ID_HEADER, PROTOCOL_MISMATCH_STATUS};

use naia_shared::transport::local::{LocalTransportHub, ServerRecvError, ServerSendError};

// ServerAuthIo - encapsulates all server auth logic (always uses hub-based multiplexing)
#[doc(hidden)]
pub struct ServerAuthIo {
    hub: LocalTransportHub,
    buffer: [u8; 1472],
    /// The fingerprint every incoming auth request must carry. The local
    /// transport is in-process and its peer is trusted in practice, but it is
    /// gated exactly like the others: a test that passes here because the
    /// check was skipped would be evidence of nothing.
    expected_protocol_id: ProtocolId,
}

impl ServerAuthIo {
    #[doc(hidden)]
    pub fn new(hub: LocalTransportHub, expected_protocol_id: ProtocolId) -> Self {
        Self {
            hub,
            buffer: [0; 1472],
            expected_protocol_id,
        }
    }

    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, ServerRecvError> {
        let Some((client_addr, request_bytes)) = self.hub.try_recv_auth_request() else {
            return Ok(None);
        };

        // Parse HTTP request
        // An unparseable request is malformed framing, not a fingerprint
        // verdict: drop it silently like any malformed credential. It must
        // never answer, never reach the application, and never be classified
        // as a protocol mismatch.
        let Ok(request) = naia_shared::transport::bytes_to_request(&request_bytes) else {
            return Ok(None);
        };

        // Compare the protocol fingerprint before anything else. Absent,
        // malformed, wrong-width and wrong-value all leave through this one
        // branch, and none of them reaches the base64 decode below, so a
        // mismatching peer's credential is never consumed.
        //
        // Unlike a malformed credential (dropped silently), a fingerprint
        // mismatch gets a prompt, payload-free, reserved-status answer so the
        // client can fail fast with exactly one `RejectEvent(ProtocolMismatch)`
        // instead of waiting indefinitely. The bytes are identical for every
        // way of failing the check and carry neither fingerprint.
        let protocol_id = request
            .headers()
            .get(PROTOCOL_ID_HEADER)
            .and_then(|value| value.to_str().ok())
            .and_then(ProtocolId::from_hex);
        if protocol_id.as_ref() != Some(&self.expected_protocol_id) {
            let response = http::Response::builder()
                .status(PROTOCOL_MISMATCH_STATUS)
                .body(Vec::new())
                .unwrap();
            let response_bytes = naia_shared::transport::response_to_bytes(response);
            let _ = self.hub.send_auth_response(&client_addr, response_bytes);
            return Ok(None);
        }

        // Extract Authorization header if present. A header that is not
        // valid ASCII, or not valid base64, is a malformed request from a
        // peer -- dropping it is right, panicking on it is not.
        let Some(auth_header) = request.headers().get("Authorization") else {
            return Ok(None);
        };
        let Ok(auth_str) = auth_header.to_str() else {
            return Ok(None);
        };
        let Ok(auth_bytes) = base64::decode(auth_str) else {
            return Ok(None);
        };
        let len = auth_bytes.len();
        if len > self.buffer.len() {
            return Ok(None);
        }
        self.buffer[0..len].copy_from_slice(&auth_bytes);
        Ok(Some((client_addr, &self.buffer[..len])))
    }

    fn accept(
        &mut self,
        address: &SocketAddr,
        identity_token: &IdentityToken,
    ) -> Result<(), ServerSendError> {
        // Build HTTP 200 response with identity token and server address in body
        let response_body = format!(
            "{}\r\n{}",
            identity_token.to_signaling_string(),
            self.hub.server_addr()
        );
        let response = http::Response::builder()
            .status(200)
            .body(response_body.into_bytes())
            .unwrap();

        let response_bytes = naia_shared::transport::response_to_bytes(response);

        // Send to the specific client via hub
        self.hub
            .send_auth_response(address, response_bytes)
            .map_err(|_| ServerSendError)?;

        Ok(())
    }

    fn reject(
        &mut self,
        address: &SocketAddr,
        payload: Option<&[u8]>,
    ) -> Result<(), ServerSendError> {
        // Build HTTP 401 response, carrying the optional rejection message
        // base64-encoded in the body (see AuthSender::reject).
        let body = match payload {
            Some(bytes) => base64::encode(bytes).into_bytes(),
            None => Vec::new(),
        };
        let response = http::Response::builder().status(401).body(body).unwrap();

        let response_bytes = naia_shared::transport::response_to_bytes(response);

        // Send to the specific client via hub
        self.hub
            .send_auth_response(address, response_bytes)
            .map_err(|_| ServerSendError)?;

        Ok(())
    }
}

#[cfg(test)]
mod local_auth_fingerprint_tests {
    use naia_shared::transport::{local::LocalTransportHub, request_to_bytes};
    use naia_shared::{ProtocolId, PROTOCOL_ID_HEADER, PROTOCOL_MISMATCH_STATUS};

    use super::ServerAuthIo;

    fn expected_id() -> ProtocolId {
        ProtocolId::new(0xdead_beef)
    }

    /// Sends one auth request carrying `fingerprint` (or none) and a valid
    /// credential, and reports whether it reached the application.
    fn request_reaches_the_application(fingerprint: Option<&str>) -> bool {
        let hub = LocalTransportHub::new("127.0.0.1:14191".parse().unwrap());
        let (_client_addr, auth_req_tx, _auth_resp_rx, _data_tx, _data_rx) = hub.register_client();
        let mut auth_io = ServerAuthIo::new(hub, expected_id());

        let mut builder = http::Request::builder()
            .method("POST")
            .uri("/")
            .header("Authorization", base64::encode([1u8, 2, 3, 4]));
        if let Some(fingerprint) = fingerprint {
            builder = builder.header(PROTOCOL_ID_HEADER, fingerprint);
        }
        let request = builder.body(Vec::new()).unwrap();
        auth_req_tx.send(request_to_bytes(request)).unwrap();

        matches!(auth_io.receive(), Ok(Some(_)))
    }

    /// Positive control: the gate is not refusing everything.
    #[test]
    fn a_matching_peer_reaches_the_application() {
        assert!(request_reaches_the_application(Some(
            &expected_id().to_hex()
        )));
    }

    /// F11/F13 on the local transport. The local transport's peer is
    /// in-process and trusted in practice, which is exactly why it is worth
    /// asserting: a gate that had been skipped here would make every local
    /// test that "passes the fingerprint check" evidence of nothing.
    ///
    /// The credential in each of these requests is valid and would decode. It
    /// is never reached -- and that is the property that matters, because a
    /// single-use identity token presented by a mismatching peer must not be
    /// consumed on its way to being refused.
    #[test]
    fn a_mismatching_peer_never_reaches_the_application() {
        let good = expected_id().to_hex();

        for (label, value) in [
            ("absent", None),
            ("wrong value", Some(ProtocolId::new(1).to_hex())),
            ("too short", Some(good[..good.len() - 1].to_string())),
            ("too long", Some(format!("{}0", good))),
            ("not hex", Some(format!("{}zz", &good[..good.len() - 2]))),
            ("empty", Some(String::new())),
        ] {
            assert!(
                !request_reaches_the_application(value.as_deref()),
                "a {label} fingerprint must be refused",
            );
        }
    }

    /// F14 -- **`require_auth = false` is still gated.**
    ///
    /// A server that auto-accepts does not read the credential at all: it mints
    /// an identity token and hands it back. The fingerprint comparison must
    /// therefore sit *upstream* of that decision, not inside the branch that
    /// reads the credential, or a mismatching peer would be auto-accepted onto
    /// a server it cannot talk to.
    ///
    /// It does sit upstream, structurally: `MainServer` reads auth requests
    /// only through `AuthReceiver::receive`, which is this `ServerAuthIo`, and
    /// the comparison happens before that call can yield anything. The test
    /// below shows the gate refusing a request that carries *no credential at
    /// all* -- the shape an auto-accepting server would otherwise be happy to
    /// serve -- which is only possible because the check does not depend on
    /// there being a credential to inspect.
    #[test]
    fn the_gate_does_not_depend_on_there_being_a_credential_to_check() {
        let hub = LocalTransportHub::new("127.0.0.1:14191".parse().unwrap());
        let (_client_addr, auth_req_tx, _auth_resp_rx, _data_tx, _data_rx) = hub.register_client();
        let mut auth_io = ServerAuthIo::new(hub, expected_id());

        let request = http::Request::builder()
            .method("POST")
            .uri("/")
            .header(PROTOCOL_ID_HEADER, ProtocolId::new(1).to_hex())
            .body(Vec::new())
            .unwrap();
        auth_req_tx.send(request_to_bytes(request)).unwrap();

        assert!(matches!(auth_io.receive(), Ok(None)));
    }

    /// A mismatch is refused with a prompt, payload-free, reserved-status
    /// answer -- not a silent drop -- so the client fails fast with exactly
    /// one `RejectEvent(ProtocolMismatch)`. Every way of failing the check
    /// gets byte-identical responses carrying neither fingerprint, and none of
    /// them reaches the application.
    #[test]
    fn a_mismatch_gets_a_prompt_payload_free_reserved_answer() {
        use naia_shared::transport::bytes_to_response;

        let good = expected_id().to_hex();
        let variants: Vec<(String, Option<String>)> = vec![
            ("absent".to_string(), None),
            ("wrong value".to_string(), Some(ProtocolId::new(1).to_hex())),
            (
                "too short".to_string(),
                Some(good[..good.len() - 1].to_string()),
            ),
            ("too long".to_string(), Some(format!("{}0", good))),
            (
                "not hex".to_string(),
                Some(format!("{}zz", &good[..good.len() - 2])),
            ),
            ("empty".to_string(), Some(String::new())),
        ];

        let mut first_bytes: Option<Vec<u8>> = None;
        for (label, value) in &variants {
            let hub = LocalTransportHub::new("127.0.0.1:14191".parse().unwrap());
            let (_client_addr, auth_req_tx, auth_resp_rx, _data_tx, _data_rx) =
                hub.register_client();
            let mut auth_io = ServerAuthIo::new(hub, expected_id());

            let mut builder = http::Request::builder()
                .method("POST")
                .uri("/")
                .header("Authorization", base64::encode([1u8, 2, 3, 4]));
            if let Some(fingerprint) = value {
                builder = builder.header(PROTOCOL_ID_HEADER, fingerprint);
            }
            let request = builder.body(Vec::new()).unwrap();
            auth_req_tx.send(request_to_bytes(request)).unwrap();

            assert!(
                matches!(auth_io.receive(), Ok(None)),
                "a {label} fingerprint must not reach the application",
            );

            let response_bytes = auth_resp_rx
                .try_recv()
                .expect("a mismatching peer must get a prompt answer, not a silent drop");
            let response = bytes_to_response(&response_bytes);
            assert_eq!(
                response.status().as_u16(),
                PROTOCOL_MISMATCH_STATUS,
                "a {label} fingerprint must get the reserved mismatch status",
            );
            assert_ne!(response.status().as_u16(), 401);
            assert!(
                response.body().is_empty(),
                "the mismatch answer must be payload-free",
            );

            match &first_bytes {
                None => first_bytes = Some(response_bytes),
                Some(first) => assert_eq!(
                    first, &response_bytes,
                    "a {label} fingerprint must be indistinguishable on the wire",
                ),
            }
        }
    }

    /// A complete malformed request is generic framing failure, not a
    /// fingerprint verdict: no answer, no application delivery, and above all
    /// no mismatch response. Every malformed shape takes this one silent path.
    #[test]
    fn a_malformed_request_is_dropped_generically_never_as_mismatch() {
        // Note: unknown-but-well-formed methods (e.g. WOBBLE) and arbitrary
        // prose with two spaces both parse as extension-method requests and
        // correctly take the mismatch branch instead -- they are NOT malformed.
        // Invalid UTF-8 in the head is framing failure too: only the
        // request-line/header region is decoded, and it is decoded strictly
        // (no lossy conversion). The body is never decoded, so garbage there
        // still parses -- see the mismatch-with-garbage-body oracle below.
        // A request with no terminator at all is incomplete framing, likewise
        // dropped. Every entry below must surface as Ok(None) -- no answer,
        // no verdict, and nothing handed to the application to allocate on.
        let malformed: Vec<Vec<u8>> = vec![
            b"GET\r\n\r\n".to_vec(),
            b"GET / HTTP/1.1\r\nNoColonHere\r\n\r\n".to_vec(),
            b"GE\x7fT / HTTP/1.1\r\nAuthorization: QUJD\r\n\r\n".to_vec(),
            b"GET {} HTTP/1.1\r\nAuthorization: QUJD\r\n\r\n".to_vec(),
            Vec::new(),
            b"GET / HTTP/1.1".to_vec(),
            b"GET /\xff HTTP/1.1\r\nAuthorization: QUJD\r\n\r\n".to_vec(),
            b"GET / HTTP/1.1\r\nX-Bad: \xff\xfe\r\n\r\n".to_vec(),
        ];

        for bytes in &malformed {
            let hub = LocalTransportHub::new("127.0.0.1:14191".parse().unwrap());
            let (_client_addr, auth_req_tx, auth_resp_rx, _data_tx, _data_rx) =
                hub.register_client();
            let mut auth_io = ServerAuthIo::new(hub, expected_id());

            auth_req_tx.send(bytes.clone()).unwrap();

            assert!(
                matches!(auth_io.receive(), Ok(None)),
                "malformed bytes must not reach the application",
            );
            assert!(
                auth_resp_rx.try_recv().is_err(),
                "malformed bytes must get no answer at all, mismatch or otherwise",
            );
        }
    }

    /// Invalid UTF-8 in the body changes nothing: only the head is decoded,
    /// so a mismatched head with a garbage body still gets the mismatch
    /// answer. This pins the terminator math -- an offset taken from a lossy
    /// conversion would slice the wrong bytes here, or panic mid-character.
    #[test]
    fn a_mismatch_with_an_invalid_utf8_body_still_gets_the_mismatch_answer() {
        use naia_shared::transport::bytes_to_response;

        let hub = LocalTransportHub::new("127.0.0.1:14191".parse().unwrap());
        let (_client_addr, auth_req_tx, auth_resp_rx, _data_tx, _data_rx) = hub.register_client();
        let mut auth_io = ServerAuthIo::new(hub, expected_id());

        let request = http::Request::builder()
            .method("POST")
            .uri("/")
            .header("Authorization", base64::encode([1u8, 2, 3, 4]))
            .header(PROTOCOL_ID_HEADER, ProtocolId::new(1).to_hex())
            .body(vec![0xff, 0xfe, 0x00, b'b', b'o', b'd', b'y'])
            .unwrap();
        auth_req_tx.send(request_to_bytes(request)).unwrap();

        assert!(
            matches!(auth_io.receive(), Ok(None)),
            "a mismatching peer must not reach the application",
        );
        let response_bytes = auth_resp_rx
            .try_recv()
            .expect("a mismatching peer must get a prompt answer, not a silent drop");
        let response = bytes_to_response(&response_bytes);
        assert_eq!(
            response.status().as_u16(),
            PROTOCOL_MISMATCH_STATUS,
            "a mismatched head with a garbage body must still get the reserved mismatch status",
        );
    }

    /// The ordering itself, asserted on the source.
    ///
    /// Every test above pins an outcome, and the outcome is the same whichever
    /// order the two checks run in: a refused peer gets `Ok(None)` either way.
    /// That is the fail-closed guarantee working, and it is exactly why
    /// behaviour cannot falsify a reordering here. The contract nevertheless
    /// requires the comparison to run *before* the credential is read and
    /// base64-decoded, so the ordering is asserted where it is visible -- in
    /// the text of the function -- mirroring the UDP backend's oracle.
    ///
    /// The body is sliced out first, so neither this test's own source nor any
    /// other function in the file can satisfy the assertion.
    #[test]
    fn the_gate_precedes_the_credential_in_the_source_not_just_in_the_outcome() {
        const THIS_FILE: &str = include_str!("auth.rs");

        let start = THIS_FILE
            .find("fn receive(&mut self)")
            .expect("ServerAuthIo::receive must exist");
        let body = &THIS_FILE[start..];
        let end = body.find("\n    }\n").expect("receive must be closed");
        let body = &body[..end];

        let gate = body
            .find("PROTOCOL_ID_HEADER")
            .expect("receive must consult the fingerprint header");
        let credential = body
            .find(r#"get("Authorization")"#)
            .expect("receive must read the credential header");

        assert!(
            gate < credential,
            "the fingerprint must be compared before the credential is read and decoded",
        );
    }
}

// LocalServerAuthSender wraps Arc<Mutex<ServerAuthIo>>
#[doc(hidden)]
#[derive(Clone)]
pub struct LocalServerAuthSender {
    auth_io: Arc<Mutex<ServerAuthIo>>,
}

impl LocalServerAuthSender {
    #[doc(hidden)]
    pub fn new(auth_io: Arc<Mutex<ServerAuthIo>>) -> Self {
        Self { auth_io }
    }

    #[doc(hidden)]
    pub fn accept(
        &self,
        address: &SocketAddr,
        identity_token: &IdentityToken,
    ) -> Result<(), ServerSendError> {
        self.auth_io.lock().accept(address, identity_token)
    }

    #[doc(hidden)]
    pub fn reject(
        &self,
        address: &SocketAddr,
        payload: Option<&[u8]>,
    ) -> Result<(), ServerSendError> {
        self.auth_io.lock().reject(address, payload)
    }
}

// LocalServerAuthReceiver wraps Arc<Mutex<ServerAuthIo>> with its own buffer
#[doc(hidden)]
#[derive(Clone)]
pub struct LocalServerAuthReceiver {
    auth_io: Arc<Mutex<ServerAuthIo>>,
    buffer: Box<[u8]>,
}

impl LocalServerAuthReceiver {
    #[doc(hidden)]
    pub fn new(auth_io: Arc<Mutex<ServerAuthIo>>) -> Self {
        Self {
            auth_io,
            buffer: Box::new([0; 1472]),
        }
    }

    #[doc(hidden)]
    pub fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, ServerRecvError> {
        let mut guard = self.auth_io.lock();
        match guard.receive() {
            Ok(option) => match option {
                Some((addr, buffer)) => {
                    self.buffer = buffer.into();
                    Ok(Some((addr, &self.buffer)))
                }
                None => Ok(None),
            },
            Err(err) => Err(err),
        }
    }
}
