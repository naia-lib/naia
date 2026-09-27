//! Native WebRTC loopback instrument: the missing sensor for the ICE/STUN
//! path behind naia-lib/naia#84, #46, #93 and #48.
//!
//! Naia's native flow configures NO STUN server anywhere (browser backends
//! hardcode `stun:stun.l.google.com:19302`; the native client builds its
//! `RTCPeerConnection` with defaults, i.e. host candidates only). So a
//! coturn-style harness cannot attach: there is no STUN address to point at
//! naia. What CAN be measured on Linux is the full peer-to-peer handshake
//! the issues are actually about — session HTTP, ICE host-candidate
//! gathering, STUN binding checks, DTLS, SCTP — by connecting a real native
//! client socket to a real server socket over loopback and asserting an
//! application echo round trip. If that handshake breaks, this goes red;
//! the refusal test below proves the red path works.
//!
//! The app side drives `listen_with_auth` and accepts: plain `listen` has
//! no auth sender, so every session request ends 401 ("missing auth
//! sender") by construction — only the auth path can complete a handshake.
//!
//! Instrument identity for every claim: native backend, 127.0.0.1 loopback,
//! instrument ports 15497-15500 (each test owns its pair; cargo runs them
//! in parallel). No sleeps without deadlines: every wait is bounded, so a
//! wedged handshake fails loudly instead of hanging the suite.

use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

use naia_client_socket::{IdentityReceiverResult, Socket as ClientSocket};
use naia_server_socket::{AuthReceiver, AuthSender, ServerAddrs, Socket as ServerSocket};
use naia_socket_shared::{IdentityToken, SocketConfig, PROTOCOL_MISMATCH_STATUS};

const PROTOCOL_ID: &str = "0000000000000000000000000000d000";
const WRONG_PROTOCOL_ID: &str = "ffffffffffffffffffffffffffffffff";

const ECHO_SESSION_PORT: u16 = 15497;
const ECHO_WEBRTC_PORT: u16 = 15498;
const REFUSE_SESSION_PORT: u16 = 15499;
const REFUSE_WEBRTC_PORT: u16 = 15500;

const HANDSHAKE_DEADLINE: Duration = Duration::from_secs(20);
const POLL_STEP: Duration = Duration::from_millis(5);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(10);

fn addrs(session_port: u16, webrtc_port: u16) -> ServerAddrs {
    ServerAddrs::new(
        SocketAddr::from(([127, 0, 0, 1], session_port)),
        SocketAddr::from(([127, 0, 0, 1], webrtc_port)),
        &format!("http://127.0.0.1:{webrtc_port}"),
    )
}

fn session_url(session_port: u16) -> String {
    format!("http://127.0.0.1:{session_port}")
}

/// Accept every incoming auth request with a fresh identity token.
fn accept_next(auth_receiver: &mut AuthReceiver, auth_sender: &AuthSender, deadline: Instant) {
    while Instant::now() < deadline {
        match auth_receiver.receive() {
            Ok(Some((addr, _))) => {
                auth_sender
                    .accept(&addr, &IdentityToken::generate())
                    .expect("app accept must reach the session listener");
                return;
            }
            Ok(None) => std::thread::sleep(POLL_STEP),
            Err(e) => panic!("auth receiver errored: {e:?}"),
        }
    }
    panic!("no auth request arrived within {HANDSHAKE_DEADLINE:?}");
}

/// The full native handshake works over loopback: session HTTP, ICE
/// gathering, STUN checks, DTLS, SCTP, then an application echo proves
/// the data channel carries bytes both ways.
#[test]
fn webrtc_loopback_echo_round_trip() {
    let addrs = addrs(ECHO_SESSION_PORT, ECHO_WEBRTC_PORT);
    let config = SocketConfig::default();
    let (auth_sender, mut auth_receiver, server_sender, mut server_receiver) =
        ServerSocket::listen_with_auth(&addrs, &config, PROTOCOL_ID);

    let (mut identity, client_sender, mut client_receiver) = ClientSocket::connect_with_auth(
        &session_url(ECHO_SESSION_PORT),
        &config,
        b"loopback".to_vec(),
        PROTOCOL_ID,
    );

    accept_next(
        &mut auth_receiver,
        &auth_sender,
        Instant::now() + HANDSHAKE_DEADLINE,
    );

    let start = Instant::now();
    loop {
        match identity.receive() {
            IdentityReceiverResult::Success(_) => break,
            IdentityReceiverResult::ErrorResponseCode(code, _) => {
                panic!("server refused a fingerprint-matching client: HTTP {code}")
            }
            IdentityReceiverResult::Waiting => {
                assert!(
                    start.elapsed() < HANDSHAKE_DEADLINE,
                    "no identity after {:?}: ICE/STUN/DTLS handshake never completed",
                    HANDSHAKE_DEADLINE
                );
                std::thread::sleep(POLL_STEP);
            }
        }
    }

    client_sender
        .send(b"PING")
        .expect("client data channel must accept a send after identity");

    let start = Instant::now();
    let client_addr = loop {
        match server_receiver.receive() {
            Ok(Some((addr, payload))) if payload == b"PING" => break addr,
            Ok(_) => {
                assert!(
                    start.elapsed() < HANDSHAKE_DEADLINE,
                    "server never got PING after {:?}",
                    HANDSHAKE_DEADLINE
                );
                std::thread::sleep(POLL_STEP);
            }
            Err(e) => panic!("server receiver errored mid-handshake: {e:?}"),
        }
    };

    server_sender
        .send(&client_addr, b"PONG")
        .expect("server must answer the client it just heard from");

    let start = Instant::now();
    loop {
        match client_receiver.receive() {
            Ok(Some(payload)) if payload == b"PONG" => break,
            Ok(_) => {
                assert!(
                    start.elapsed() < HANDSHAKE_DEADLINE,
                    "client never got PONG after {:?}",
                    HANDSHAKE_DEADLINE
                );
                std::thread::sleep(POLL_STEP);
            }
            Err(e) => panic!("client receiver errored mid-handshake: {e:?}"),
        }
    }

    assert!(
        ServerSocket::close_with_auth(
            (auth_sender, auth_receiver, server_sender, server_receiver),
            &addrs,
            CLOSE_TIMEOUT
        ),
        "loopback ports must be released by close"
    );
}

/// The fingerprint gate fires before any credential is examined, with the
/// reserved mismatch status — so a mismatched client is refused 409 even
/// with no auth at all. This is the committed red control for the
/// instrument: it fails if refusal ever silently stops working, and the
/// echo test above was shown to go red (401/never-connected) before the
/// auth-driven accept landed.
#[test]
fn webrtc_loopback_wrong_fingerprint_is_refused() {
    let addrs = addrs(REFUSE_SESSION_PORT, REFUSE_WEBRTC_PORT);
    let config = SocketConfig::default();
    let (server_sender, server_receiver) = ServerSocket::listen(&addrs, &config, PROTOCOL_ID);

    let (mut identity, _client_sender, _client_receiver) = ClientSocket::connect(
        &session_url(REFUSE_SESSION_PORT),
        &config,
        WRONG_PROTOCOL_ID,
    );

    let start = Instant::now();
    loop {
        match identity.receive() {
            IdentityReceiverResult::ErrorResponseCode(code, _) => {
                assert_eq!(
                    code, PROTOCOL_MISMATCH_STATUS,
                    "mismatch must carry the reserved status, not a generic refusal"
                );
                break;
            }
            IdentityReceiverResult::Success(_) => {
                panic!("server accepted a fingerprint-mismatched client")
            }
            IdentityReceiverResult::Waiting => {
                assert!(
                    start.elapsed() < HANDSHAKE_DEADLINE,
                    "mismatched client neither refused nor accepted after {:?}",
                    HANDSHAKE_DEADLINE
                );
                std::thread::sleep(POLL_STEP);
            }
        }
    }

    assert!(
        ServerSocket::close((server_sender, server_receiver), &addrs, CLOSE_TIMEOUT),
        "refusal-test ports must be released by close"
    );
}
