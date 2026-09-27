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
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
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
const PROXY_SESSION_PORT: u16 = 15501;

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

/// Logging forward proxy for the session HTTP exchange. The client aims at
/// the proxy; every request is forwarded byte-identical to the real session
/// listener and the response relayed back. POST bodies (client SDP offers)
/// and their response bodies (server SDP answers) are captured for
/// [`assert_host_only_candidates`]: the client's offers carry NO candidates
/// at all in this stack (measured: zero `a=candidate:` lines, no trickle
/// channel) and the server answers ICE-lite with zero candidates as well,
/// so the sensor pins srflx/relay ABSENCE over a non-vacuous SDP capture.
/// Exits when `stop` is set; the
/// accept loop is nonblocking so the thread always joins on shutdown.
fn start_session_proxy(
    real_session_port: u16,
    proxy_port: u16,
    captured: Arc<Mutex<Vec<Vec<u8>>>>,
    stop: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let listener =
            TcpListener::bind(("127.0.0.1", proxy_port)).expect("proxy must bind its port");
        listener
            .set_nonblocking(true)
            .expect("proxy listener must be nonblocking");
        while !stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((client, _)) => {
                    let captured = Arc::clone(&captured);
                    std::thread::spawn(move || {
                        forward_one(client, real_session_port, &captured);
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(POLL_STEP);
                }
                Err(_) => break,
            }
        }
    })
}

fn read_http_message(stream: &mut TcpStream) -> Option<(Vec<u8>, Vec<u8>)> {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .ok()?;
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if stream.read_exact(&mut byte).is_err() || head.len() > 65536 {
            return None;
        }
        head.extend_from_slice(&byte);
    }
    let head_text = String::from_utf8_lossy(&head);
    let content_len = head_text.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        (k.trim().eq_ignore_ascii_case("content-length"))
            .then(|| v.trim().parse::<usize>().unwrap_or(0))
    });
    let mut body = Vec::new();
    match content_len {
        Some(n) => {
            body.resize(n, 0);
            if n > 0 {
                stream.read_exact(&mut body).ok()?;
            }
        }
        // No length: close-delimited. Read to EOF (the peer closes).
        None => {
            let mut chunk = [0u8; 4096];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => body.extend_from_slice(&chunk[..n]),
                    Err(_) => break,
                }
            }
        }
    }
    Some((head, body))
}

fn forward_one(mut client: TcpStream, real_session_port: u16, captured: &Arc<Mutex<Vec<Vec<u8>>>>) {
    let (head, body) = match read_http_message(&mut client) {
        Some(m) => m,
        None => return,
    };
    let is_post = head.starts_with(b"POST");
    if is_post && !body.is_empty() {
        captured.lock().unwrap().push(body.clone());
    }
    let mut server = match TcpStream::connect(("127.0.0.1", real_session_port)) {
        Ok(s) => s,
        Err(_) => return,
    };
    let _ = server.set_read_timeout(Some(Duration::from_secs(10)));
    if server.write_all(&head).is_err() || server.write_all(&body).is_err() {
        return;
    }
    if let Some((resp_head, resp_body)) = read_http_message(&mut server) {
        let _ = client.write_all(&resp_head);
        let _ = client.write_all(&resp_body);
        if is_post && !resp_body.is_empty() {
            captured.lock().unwrap().push(resp_body);
        }
    }
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

    let captured: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let stop_proxy = Arc::new(AtomicBool::new(false));
    let proxy = start_session_proxy(
        ECHO_SESSION_PORT,
        PROXY_SESSION_PORT,
        Arc::clone(&captured),
        Arc::clone(&stop_proxy),
    );

    let (mut identity, client_sender, mut client_receiver) = ClientSocket::connect_with_auth(
        &session_url(PROXY_SESSION_PORT),
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

    // The client's own session SDP must show host candidates and no srflx
    // or relay: the measured absence behind the NAT diagnosis.
    let bodies = captured.lock().unwrap();
    assert_host_only_candidates(&bodies);

    stop_proxy.store(true, Ordering::SeqCst);
    let proxy_start = Instant::now();
    while !proxy.is_finished() {
        assert!(
            proxy_start.elapsed() < CLOSE_TIMEOUT,
            "proxy thread did not exit after stop"
        );
        std::thread::sleep(POLL_STEP);
    }
    proxy.join().expect("proxy thread panicked");

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

/// Candidate types seen in session SDP bodies (`a=candidate:` lines).
/// SDP may arrive JSON-escaped, so both real and literal `\r\n` split lines.
fn gathered_candidate_types(session_bodies: &[Vec<u8>]) -> Vec<String> {
    let mut types = Vec::new();
    for body in session_bodies {
        let text = String::from_utf8_lossy(body).replace("\\r\\n", "\n");
        for line in text.replace("\r\n", "\n").lines() {
            let line = line.trim().trim_end_matches("\\n").trim();
            let Some(rest) = line.strip_prefix("a=candidate:") else {
                continue;
            };
            let parts = rest.split_whitespace();
            if parts.clone().any(|p| p == "typ") {
                let typ = parts
                    .skip_while(|p| *p != "typ")
                    .nth(1)
                    .unwrap_or("unknown");
                types.push(typ.to_string());
            }
        }
    }
    types
}

/// The srflx-absence sensor: no server-reflexive or relayed candidate may
/// appear anywhere in the session SDP exchange. Measured on this stack the
/// exchange is an ICE-lite answer with ZERO `a=candidate:` lines on either
/// side (the client offer carries no candidates and no trickle channel; the
/// server answers `a=ice-lite` and the checks address the m=/c= connection
/// address directly), so requiring a host candidate is unsatisfiable —
/// non-vacuity is instead pinned on observing at least one SDP payload.
/// Panics on any srflx/relay candidate, and on zero observed SDP (a vacuous
/// pass would hide a broken capture, not a clean gather). If someone adds a
/// STUN default to the native client, this fails loudly instead of silently
/// invalidating every NAT diagnosis.
fn assert_host_only_candidates(session_bodies: &[Vec<u8>]) {
    let types = gathered_candidate_types(session_bodies);
    assert!(
        types
            .iter()
            .all(|t| t.as_str() != "srflx" && t.as_str() != "relay"),
        "native gather must show no srflx/relay candidates, saw: {types:?}"
    );
    let saw_sdp = session_bodies
        .iter()
        .any(|b| String::from_utf8_lossy(b).contains("sdp"));
    assert!(
        saw_sdp,
        "captured no SDP payload in the session exchange: the sensor is blind, not green"
    );
}

#[test]
fn candidate_classifier_accepts_host_only_exchange() {
    let body = br#"{"sdp":"v=0\r\na=candidate:1 1 udp 2113937151 127.0.0.1 5000 typ host\r\na=candidate:2 1 udp 2113937150 192.168.1.5 5001 typ host\r\n"}"#
        .to_vec();
    assert_host_only_candidates(&[body]);
}

#[test]
fn candidate_classifier_accepts_ice_lite_answer_without_candidates() {
    // The measured live shape: ICE-lite answer, zero a=candidate lines.
    let body = br#"{"sdp":{"answer":{"sdp":"v=0\r\nc=IN IP4 127.0.0.1\r\na=ice-lite\r\na=ice-ufrag:QYGt\r\n"}}}"#
        .to_vec();
    assert_host_only_candidates(&[body]);
}

#[test]
#[should_panic(expected = "no srflx/relay")]
fn candidate_classifier_rejects_injected_srflx() {
    let body = br#"{"sdp":"v=0\r\na=candidate:1 1 udp 2113937151 127.0.0.1 5000 typ host\r\na=candidate:2 1 udp 1685987071 203.0.113.7 5001 typ srflx raddr 192.168.1.5 rport 5001\r\n"}"#
        .to_vec();
    assert_host_only_candidates(&[body]);
}

#[test]
#[should_panic(expected = "no srflx/relay")]
fn candidate_classifier_rejects_injected_relay() {
    let body = br#"{"sdp":"v=0
a=candidate:1 1 udp 2113937151 127.0.0.1 5000 typ host
a=candidate:3 1 udp 41885439 198.51.100.9 3478 typ relay raddr 203.0.113.7 rport 5001
"}"#
    .to_vec();
    assert_host_only_candidates(&[body]);
}

#[test]
#[should_panic(expected = "no SDP payload")]
fn candidate_classifier_rejects_empty_exchange() {
    assert_host_only_candidates(&[br#"{"heartbeat":true}"#.to_vec()]);
}
