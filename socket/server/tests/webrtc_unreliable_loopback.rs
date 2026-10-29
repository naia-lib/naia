//! Hermetic loopback interop tests: webrtc-unreliable-client <-> webrtc-unreliable
//! (via naia-server-socket), exercising the full signaling + DTLS + SCTP path
//! in-process on 127.0.0.1. Bounded,
//! self-cleaning, and assertion-driven.
//!
//! Each test uses its own port pair so tests can run
//! concurrently.
//! Any WARN/ERROR logs emitted during the run are captured and reported at the
//! end as unresolved evidence — never filtered, but only connection-path
//! failures fail the test.
//!
//! Beyond the happy-path baseline, the variants below deliberately broaden the
//! exercised code envelope (for coverage-guided slimming of the vendored
//! webrtc tree): large payloads (SCTP fragmentation/reassembly), auth
//! rejection, and abrupt client teardown.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread,
    time::Duration,
};

use futures_util::future::{select, Either};
use naia_server_socket::{
    shared::{stamp_protocol_id_header, IdentityToken, SocketConfig},
    ServerAddrs, Socket as ServerSocket,
};
use webrtc_unreliable_client::{SessionError, Socket as ClientSocket};

const AUTH_TOKEN: &str = "12345";
/// Protocol fingerprint the test client stamps and the test server expects
/// (naia server `listen_with_auth` refuses requests without an exact match).
const PROTOCOL_ID: &str = "00112233445566778899aabbccddeeff";
/// Stand-in for a serialized naia rejection message (naia-lib/naia#133).
/// The server sends it base64-encoded in the 401 body.
const REJECT_REASON: &[u8] = b"bad token, friend";
const WHOLE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const PER_REPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// Test-only timeout: `Err(Elapsed)` when `duration` passes before `fut`.
#[derive(Debug)]
struct Elapsed;

async fn timeout<T>(
    duration: Duration,
    fut: impl std::future::Future<Output = T>,
) -> Result<T, Elapsed> {
    match select(Box::pin(smol::Timer::after(duration)), Box::pin(fut)).await {
        Either::Left(_) => Err(Elapsed),
        Either::Right((out, _)) => Ok(out),
    }
}

async fn timeout_at<T>(
    deadline: std::time::Instant,
    fut: impl std::future::Future<Output = T>,
) -> Result<T, Elapsed> {
    timeout(
        deadline.saturating_duration_since(std::time::Instant::now()),
        fut,
    )
    .await
}

/// Captures WARN/ERROR records from the whole process so teardown artifacts
/// (e.g. the known SCTP heartbeat parse warning) are surfaced, not filtered.
struct CapturingLogger {
    warnings: Mutex<Vec<String>>,
}

static LOGGER: OnceLock<CapturingLogger> = OnceLock::new();

impl log::Log for &'static CapturingLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Warn
    }
    fn log(&self, record: &log::Record) {
        if record.level() <= log::Level::Warn {
            self.warnings
                .lock()
                .unwrap()
                .push(format!("{}: {}", record.target(), record.args()));
        }
    }
    fn flush(&self) {}
}

fn install_logger() -> &'static CapturingLogger {
    let logger = LOGGER.get_or_init(|| CapturingLogger {
        warnings: Mutex::new(Vec::new()),
    });
    let _ = log::set_logger(Box::leak(Box::new(logger)));
    log::set_max_level(log::LevelFilter::Warn);
    logger
}

/// Server half: auth-accept (token match) + echo, polled on a thread until `stop`.
/// Echoes every received payload back verbatim and counts echoes.
fn run_server(signal_addr: &str, data_addr: &str, stop: Arc<AtomicBool>, echoes: Arc<AtomicU32>) {
    let server_addrs = ServerAddrs::new(
        signal_addr.parse().expect("bad session addr"),
        data_addr.parse().expect("bad data addr"),
        &format!("http://{}", data_addr),
    );
    run_server_with_addrs(server_addrs, stop, echoes)
}

fn run_server_with_addrs(server_addrs: ServerAddrs, stop: Arc<AtomicBool>, echoes: Arc<AtomicU32>) {
    let (auth_sender, mut auth_receiver, packet_sender, mut packet_receiver) =
        ServerSocket::listen_with_auth(&server_addrs, &SocketConfig::new(None, None), PROTOCOL_ID);

    while !stop.load(Ordering::Relaxed) {
        let mut idle = true;

        if let Ok(Some((address, payload))) = auth_receiver.receive() {
            idle = false;
            if String::from_utf8_lossy(payload) == AUTH_TOKEN {
                auth_sender
                    .accept(&address, &IdentityToken::generate())
                    .expect("server failed to accept auth");
            } else {
                let _ = auth_sender.reject(&address, Some(REJECT_REASON));
            }
        }

        if let Ok(Some((address, payload))) = packet_receiver.receive() {
            idle = false;
            let payload = payload.to_vec();
            packet_sender
                .send(&address, &payload)
                .expect("server failed to echo");
            echoes.fetch_add(1, Ordering::Relaxed);
        }

        if idle {
            thread::sleep(Duration::from_millis(1));
        }
    }
}

struct ServerHandle {
    stop: Arc<AtomicBool>,
    echoes: Arc<AtomicU32>,
    thread: thread::JoinHandle<()>,
}

fn spawn_server(signal_addr: &'static str, data_addr: &'static str) -> ServerHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let echoes = Arc::new(AtomicU32::new(0));
    let thread = {
        let stop = stop.clone();
        let echoes = echoes.clone();
        thread::spawn(move || run_server(signal_addr, data_addr, stop, echoes))
    };
    ServerHandle {
        stop,
        echoes,
        thread,
    }
}

impl ServerHandle {
    fn shutdown(self) -> u32 {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.join().expect("server thread panicked");
        self.echoes.load(Ordering::Relaxed)
    }
}

/// Connects a client and waits for the accepted id token; asserts the data
/// addr resolved. Returns the io halves for the caller to drive.
async fn connect_client(
    signal_addr: &str,
    token: &str,
) -> Result<webrtc_unreliable_client::SocketIo, SessionError> {
    let (socket, mut io) = ClientSocket::new();
    socket
        .connect(
            &format!("http://{}/rtc_session", signal_addr),
            Some(token.as_bytes().to_vec()),
            Some(stamp_protocol_id_header(None, PROTOCOL_ID)),
        )
        .await;

    let id_result = timeout(Duration::from_secs(10), &mut io.to_client_id_receiver)
        .await
        .expect("timed out waiting for auth outcome (signaling/DTLS handshake)")
        .expect("id channel dropped");
    match id_result {
        Ok(id) => {
            assert!(!id.is_empty(), "empty id token");
            // connect() has run to completion above, so the candidate has been
            // parsed and the address is already sitting in the channel.
            io.to_client_addr_receiver
                .try_recv()
                .expect("server data addr not resolved after handshake");
            Ok(io)
        }
        Err(err) => Err(err),
    }
}

/// Sends each payload and asserts it is echoed back verbatim.
async fn echo_rounds(io: &mut webrtc_unreliable_client::SocketIo, payloads: &[Vec<u8>]) {
    for (i, payload) in payloads.iter().enumerate() {
        io.to_server_sender
            .try_send(payload.clone().into_boxed_slice())
            .expect("client send channel closed");
        let reply = timeout(PER_REPLY_TIMEOUT, io.to_client_receiver.recv())
            .await
            .unwrap_or_else(|_| panic!("round {i}: no echo within {PER_REPLY_TIMEOUT:?}"))
            .unwrap_or_else(|_| panic!("round {i}: client receive channel closed"));
        assert_eq!(
            reply.as_ref(),
            payload.as_slice(),
            "round {i}: echoed payload mismatch (len sent {} vs received {})",
            payload.len(),
            reply.len()
        );
    }
}

fn report_warnings(logger: &CapturingLogger, label: &str, elapsed: Duration) {
    let warnings = logger.warnings.lock().unwrap();
    println!(
        "{label}: done in {elapsed:?}; {} WARN+ log records captured so far \
         (unresolved evidence, cumulative across tests, not filtered):",
        warnings.len()
    );
    for w in warnings.iter() {
        println!("  UNRESOLVED-EVIDENCE {w}");
    }
}

/// Deterministic lossy/reordering UDP relay between client and server data
/// ports: drops datagram #10 (mid-handshake) and #60 (data phase), and
/// delays ~1/7 of packets by 80ms (reordering them past successors), in both
/// directions. Loopback never drops packets,
/// so this is what forces the transport-corrective paths (DTLS handshake
/// flight retransmission, STUN ping retries, SCTP SACK gap handling /
/// forward-TSN) to actually run.
async fn run_lossy_proxy(listen: &str, server: &str) {
    let sock = std::sync::Arc::new(
        async_net::UdpSocket::bind(listen)
            .await
            .expect("proxy bind failed"),
    );
    let server_addr: std::net::SocketAddr = server.parse().unwrap();
    let mut client_addr: Option<std::net::SocketAddr> = None;
    let mut counter = 0u64;
    // Max UDP payload: the DTLS certificate flight (4096-bit RSA cert in
    // webrtc-unreliable 0.6.0) exceeds a typical-MTU-sized buffer, and
    // recv_from silently truncates.
    let mut buf = vec![0u8; 65536];
    loop {
        let Ok((n, from)) = sock.recv_from(&mut buf).await else {
            return;
        };
        counter += 1;
        // Calibration evidence (2026-08-27): one dropped handshake packet
        // costs ~7s of retransmission backoff; sustained aperiodic loss of
        // even 6.25% keeps the handshake from completing within 30s; and a
        // periodic every-Nth drop resonates with flight retransmission (a
        // 16-packet handshake cycle re-lost the same packets forever). Hence
        // a two-packet deterministic drop: recovery runs, test stays bounded.
        let mix = counter
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let roll = mix >> 33;
        if counter == 10 || counter == 60 {
            continue; // dropped
        }
        let dest = if from == server_addr {
            match client_addr {
                Some(addr) => addr,
                None => continue,
            }
        } else {
            client_addr = Some(from);
            server_addr
        };
        if roll.is_multiple_of(7) {
            let sock = sock.clone();
            let data = buf[..n].to_vec();
            smol::spawn(async move {
                smol::Timer::after(Duration::from_millis(80)).await;
                let _ = sock.send_to(&data, dest).await;
            })
            .detach();
        } else {
            let _ = sock.send_to(&buf[..n], dest).await;
        }
    }
}

/// Lossy link: the server advertises the proxy's port as its public address,
/// so all DTLS/SCTP traffic crosses the dropping/reordering relay. The
/// connection must still establish (handshake retransmission) and a usable
/// fraction of unreliable messages must still round-trip.
#[test]
fn loopback_lossy_link() {
    let logger = install_logger();
    // Server listens on 24232 but advertises the proxy at 24233.
    let server = {
        let stop = Arc::new(AtomicBool::new(false));
        let echoes = Arc::new(AtomicU32::new(0));
        let thread = {
            let stop = stop.clone();
            let echoes = echoes.clone();
            thread::spawn(move || {
                let server_addrs = ServerAddrs::new(
                    "127.0.0.1:24231".parse().unwrap(),
                    "127.0.0.1:24232".parse().unwrap(),
                    "http://127.0.0.1:24233",
                );
                run_server_with_addrs(server_addrs, stop, echoes)
            })
        };
        ServerHandle {
            stop,
            echoes,
            thread,
        }
    };
    let started = std::time::Instant::now();

    const SENT: u32 = 20;
    let received = smol::block_on(async {
        let proxy = smol::spawn(run_lossy_proxy("127.0.0.1:24233", "127.0.0.1:24232"));
        let received = timeout(WHOLE_TEST_TIMEOUT, async {
            let io = connect_client("127.0.0.1:24231", AUTH_TOKEN)
                .await
                .expect("auth unexpectedly rejected");
            // The id token arrives via HTTP signaling, so it does not prove
            // the DTLS/SCTP handshake finished. Probe with PROBE messages
            // (lossy link: any single probe or its echo may vanish) until the
            // first echo proves the data channel is open end-to-end.
            loop {
                io.to_server_sender
                    .try_send(b"PROBE".to_vec().into_boxed_slice())
                    .expect("client send channel closed");
                match timeout(Duration::from_millis(250), io.to_client_receiver.recv()).await {
                    Ok(Ok(reply)) if reply.as_ref() == b"PROBE" => break,
                    Ok(Ok(other)) => panic!("unexpected probe reply: {other:?}"),
                    Ok(Err(_)) => panic!("client receive channel closed during probe"),
                    Err(_) => {} // lost probe or echo; retry
                }
            }

            // Unreliable transport over a 25%-loss link: count what survives.
            let mut received = 0u32;
            for _ in 0..SENT {
                io.to_server_sender
                    .try_send(b"PING".to_vec().into_boxed_slice())
                    .expect("client send channel closed");
                smol::Timer::after(Duration::from_millis(50)).await;
            }
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while let Ok(Ok(reply)) = timeout_at(deadline, io.to_client_receiver.recv()).await {
                if reply.as_ref() == b"PROBE" {
                    continue; // straggler echo from the handshake probe phase
                }
                assert_eq!(reply.as_ref(), b"PING", "echoed payload corrupted");
                received += 1;
            }
            let _ = io.to_server_disconnect_sender.send(()).await;
            received
        })
        .await
        .expect("whole-test timeout exceeded (connection never established?)");
        drop(proxy); // detach: the proxy task is process-scoped, like the test
        received
    });
    let echoes = server.shutdown();

    println!("lossy-link: {received}/{SENT} echoes ({echoes} sent by server)");
    // ~56% of round trips survive a 25%-per-direction loss link in
    // expectation; require a loose floor, and no phantom extras.
    assert!(
        received >= SENT / 4,
        "only {received}/{SENT} echoes survived the lossy link"
    );
    // `echoes` also counts PROBE echoes from the handshake phase, so it only
    // lower-bounds `received`.
    assert!(received <= echoes);
    report_warnings(logger, "lossy-link", started.elapsed());
}

/// Baseline: 5 bidirectional PING/PONG round trips, deterministic teardown.
#[test]
fn loopback_ping_pong_round_trips() {
    let logger = install_logger();
    let server = spawn_server("127.0.0.1:24191", "127.0.0.1:24192");
    let started = std::time::Instant::now();

    let result = smol::block_on(async {
        timeout(WHOLE_TEST_TIMEOUT, async {
            let mut io = connect_client("127.0.0.1:24191", AUTH_TOKEN)
                .await
                .expect("auth unexpectedly rejected");
            let payloads: Vec<Vec<u8>> = (0..5).map(|_| b"PING".to_vec()).collect();
            echo_rounds(&mut io, &payloads).await;
            // Deterministic client-side shutdown before the server stops.
            let _ = io.to_server_disconnect_sender.send(()).await;
            smol::Timer::after(Duration::from_millis(250)).await;
        })
        .await
    });
    let echoes = server.shutdown();

    result.expect("whole-test timeout exceeded");
    assert_eq!(echoes, 5, "server-side echo count mismatch");
    report_warnings(logger, "baseline", started.elapsed());
}

/// Large payloads, up to the system's supported envelope: webrtc-unreliable
/// (the only peer) drops fragmented SCTP packets by design (client.rs:614 in
/// that repo), so a message must fit in a single SCTP DATA chunk — at most
/// `MAX_MESSAGE_SIZE` (1200) bytes. Sizes up to the cap must round-trip;
/// sizes over the cap must be REJECTED at the client send boundary (logged
/// error, message dropped locally, connection unharmed) rather than silently
/// fragmented and lost on the wire.
#[test]
fn loopback_large_payloads_round_trip() {
    let logger = install_logger();
    let server = spawn_server("127.0.0.1:24201", "127.0.0.1:24202");
    let started = std::time::Instant::now();

    let result = smol::block_on(async {
        timeout(WHOLE_TEST_TIMEOUT, async {
            let mut io = connect_client("127.0.0.1:24201", AUTH_TOKEN)
                .await
                .expect("auth unexpectedly rejected");
            let payloads: Vec<Vec<u8>> = [
                512usize,
                1000,
                1150,
                webrtc_unreliable_client::MAX_MESSAGE_SIZE,
            ]
            .iter()
            .map(|&n| (0..n).map(|i| (i % 251) as u8).collect())
            .collect();
            let mut count = payloads.len() as u32;
            echo_rounds(&mut io, &payloads).await;

            // Oversize sends: must be rejected client-side with a logged
            // error and never reach the server.
            let rejections_before = count_oversize_rejections(logger);
            for n in [webrtc_unreliable_client::MAX_MESSAGE_SIZE + 1, 4000usize] {
                let oversize: Vec<u8> = vec![0xAB; n];
                io.to_server_sender
                    .try_send(oversize.into_boxed_slice())
                    .expect("client send channel closed");
            }
            // The connection must survive: a small follow-up still echoes,
            // and it is the NEXT thing echoed (the oversize ones never went).
            echo_rounds(&mut io, &[b"after-oversize".to_vec()]).await;
            count += 1;
            assert_eq!(
                count_oversize_rejections(logger) - rejections_before,
                2,
                "expected both oversize sends to log a client-side rejection"
            );

            let _ = io.to_server_disconnect_sender.send(()).await;
            smol::Timer::after(Duration::from_millis(250)).await;
            count
        })
        .await
    });
    let echoes = server.shutdown();

    let count = result.expect("whole-test timeout exceeded");
    assert_eq!(echoes, count, "server-side echo count mismatch");
    report_warnings(logger, "large-payloads", started.elapsed());
}

fn count_oversize_rejections(logger: &CapturingLogger) -> usize {
    logger
        .warnings
        .lock()
        .unwrap()
        .iter()
        .filter(|w| w.contains("exceeds MAX_MESSAGE_SIZE"))
        .count()
}

/// Wrong token: the server must reject, and the client must observe it.
#[test]
fn loopback_auth_reject() {
    let logger = install_logger();
    let server = spawn_server("127.0.0.1:24211", "127.0.0.1:24212");
    let started = std::time::Instant::now();

    let result = smol::block_on(async {
        timeout(WHOLE_TEST_TIMEOUT, async {
            connect_client("127.0.0.1:24211", "wrong-token").await
        })
        .await
    });
    let echoes = server.shutdown();

    let outcome = result.expect("whole-test timeout exceeded");
    assert!(outcome.is_err(), "server accepted a wrong auth token");
    assert_eq!(echoes, 0, "no data should flow on a rejected session");
    report_warnings(logger, "auth-reject", started.elapsed());
}

/// A rejection may carry an application-defined reason in its body. Reporting
/// only the status code would throw that away, leaving the client unable to
/// tell the user *why* it was refused (naia-lib/naia#133).
#[test]
fn loopback_auth_reject_carries_the_server_reason() {
    let logger = install_logger();
    let server = spawn_server("127.0.0.1:24215", "127.0.0.1:24216");
    let started = std::time::Instant::now();

    let result = smol::block_on(async {
        timeout(WHOLE_TEST_TIMEOUT, async {
            connect_client("127.0.0.1:24215", "wrong-token").await
        })
        .await
    });
    server.shutdown();

    let outcome = result.expect("whole-test timeout exceeded");
    let error = outcome.err().expect("server accepted a wrong auth token");
    assert_eq!(error.status_code, 401, "unexpected rejection status");
    assert_eq!(
        base64::decode(error.body.trim()).expect("rejection body was not base64"),
        REJECT_REASON,
        "the server's reason did not survive the trip to the client"
    );
    report_warnings(logger, "auth-reject-reason", started.elapsed());
}

/// Abrupt teardown: client vanishes mid-session (no disconnect message).
/// The server must stay healthy and remain usable; exercises client-loss
/// paths on the server and non-graceful shutdown paths in the client.
#[test]
fn loopback_abrupt_client_teardown() {
    let logger = install_logger();
    let server = spawn_server("127.0.0.1:24221", "127.0.0.1:24222");
    let started = std::time::Instant::now();

    let result = smol::block_on(async {
        timeout(WHOLE_TEST_TIMEOUT, async {
            let mut io = connect_client("127.0.0.1:24221", AUTH_TOKEN)
                .await
                .expect("auth unexpectedly rejected");
            echo_rounds(&mut io, &[b"PING".to_vec()]).await;
            // Drop io without sending disconnect: abrupt vanish.
            drop(io);
        })
        .await
    });
    result.expect("whole-test timeout exceeded");

    // Server must still be alive and polling after the client vanished.
    thread::sleep(Duration::from_millis(500));
    assert!(
        !server.thread.is_finished(),
        "server thread died after abrupt client teardown"
    );
    let echoes = server.shutdown();
    assert_eq!(echoes, 1);
    report_warnings(logger, "abrupt-teardown", started.elapsed());
}
