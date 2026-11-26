use std::net::SocketAddr;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

#[cfg(feature = "test_time")]
use naia_shared::TestClock;
use naia_shared::{DisconnectReason, GameInstant, IdentityToken, Instant, Protocol};

#[cfg(feature = "test_time")]
use crate::world_events::{DisconnectEvent, WorldEvent};

use crate::connection::time_manager::TimeManager;
use crate::transport::{
    IdentityReceiver, IdentityReceiverResult, PacketReceiver, PacketSender, RecvError, SendError,
    ServerAddr,
};

use super::*;

fn dummy_server() -> ServerAddr {
    ServerAddr::Found("127.0.0.1:9999".parse::<SocketAddr>().unwrap())
}

#[derive(Clone)]
struct OkIdReceiver;

impl IdentityReceiver for OkIdReceiver {
    fn receive(&mut self) -> IdentityReceiverResult {
        IdentityReceiverResult::Success(IdentityToken::generate())
    }
}

#[derive(Clone)]
struct CountingSender {
    sent: Arc<AtomicUsize>,
}

impl PacketSender for CountingSender {
    fn send(&self, _payload: &[u8]) -> Result<(), SendError> {
        self.sent.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn server_addr(&self) -> ServerAddr {
        dummy_server()
    }

    fn shutdown(&mut self) {}
}

#[derive(Clone)]
struct EmptyReceiver;

impl PacketReceiver for EmptyReceiver {
    fn receive(&mut self) -> Result<Option<&[u8]>, RecvError> {
        Ok(None)
    }

    fn server_addr(&self) -> ServerAddr {
        dummy_server()
    }
}

fn idle_client() -> Client<u64> {
    Client::<u64>::new(ClientConfig::default(), Protocol::builder().build())
}

fn loading_client(sender: CountingSender) -> Client<u64> {
    loading_client_with_receiver(Box::new(sender), Box::new(EmptyReceiver))
}

fn loading_client_with_receiver(
    sender: Box<dyn PacketSender>,
    receiver: Box<dyn PacketReceiver>,
) -> Client<u64> {
    let mut client = idle_client();
    client.io.load(Box::new(OkIdReceiver), sender, receiver);
    client
}

/// A client with an established connection, without a live server: the
/// connection is built exactly the way `maintain_handshake` builds it on
/// `HandshakeResult::Connected`, with pristine (zero-sample) timing.
fn connected_client() -> (Client<u64>, Arc<AtomicUsize>) {
    let sent = Arc::new(AtomicUsize::new(0));
    let mut client = loading_client(CountingSender { sent: sent.clone() });
    client
        .handshake_manager
        .set_identity_token(IdentityToken::generate());
    // A real handshake always sets the challenge digest before
    // reaching Connected; the fake above must carry it too, or UDP
    // graceful-disconnect signing (infallible in production) panics.
    // No-op off the UDP build.
    client
        .handshake_manager
        .seed_challenge_digest_for_tests(vec![0u8; 32]);
    let time_manager = TimeManager::from_parts(
        Duration::from_millis(100),
        BaseTimeManager::new(),
        0,
        GameInstant::new(&Instant::now()),
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
    );
    client.server_connection = Some(Connection::new(
        &client.client_config.connection,
        &client.protocol.channel_kinds,
        time_manager,
        &client.global_world_manager,
        client.client_config.jitter_buffer,
        &client.protocol.component_kinds,
    ));
    (client, sent)
}

#[test]
#[should_panic(expected = "not connected yet")]
fn disconnect_panics_when_not_connected() {
    let mut client = idle_client();
    client.disconnect();
}

#[test]
fn cancel_connect_on_idle_stays_disconnected() {
    let mut client = idle_client();
    assert_eq!(client.connection_status(), ConnectionStatus::Disconnected);
    client.cancel_connect();
    assert_eq!(client.connection_status(), ConnectionStatus::Disconnected);
}

#[test]
fn cancel_connect_abandons_pending_handshake() {
    let sent = Arc::new(AtomicUsize::new(0));
    let mut client = loading_client(CountingSender { sent: sent.clone() });
    assert_eq!(client.connection_status(), ConnectionStatus::Connecting);
    client.cancel_connect();
    assert_eq!(client.connection_status(), ConnectionStatus::Disconnected);
    // Abandoning sends nothing: teardown of an unestablished attempt is
    // silent (no graceful-disconnect packets for a connection that never
    // existed).
    assert_eq!(sent.load(Ordering::SeqCst), 0);
}

#[test]
fn cancel_connect_leaves_live_connection_alone() {
    let (mut client, _sent) = connected_client();
    assert_eq!(client.connection_status(), ConnectionStatus::Connected);
    client.cancel_connect();
    assert_eq!(client.connection_status(), ConnectionStatus::Connected);
    assert!(client.server_connection.is_some());
}

#[test]
fn disconnect_sends_ten_packets_then_disconnecting() {
    let (mut client, sent) = connected_client();
    client.disconnect();
    assert_eq!(
        sent.load(Ordering::SeqCst),
        10,
        "graceful disconnect must emit exactly 10 packets",
    );
    assert_eq!(client.connection_status(), ConnectionStatus::Disconnecting);
}

#[test]
fn server_named_reason_wins_over_local_flags() {
    // naia-lib/naia#10: a server-initiated disconnect names its own
    // reason; falling back to ClientDisconnected would tell the client
    // it hung up on itself — even when the manual flag is also set.
    let payload = Some(vec![7u8; 4]);
    let (reason, out_payload) = Client::<u64>::resolve_disconnect_reason(
        Some((DisconnectReason::Kicked, payload.clone())),
        true,
        true,
    );
    assert_eq!(reason, DisconnectReason::Kicked);
    assert_eq!(out_payload, payload);
}

#[test]
fn manual_disconnect_maps_to_client_disconnected() {
    let (reason, payload) = Client::<u64>::resolve_disconnect_reason(None, true, false);
    assert_eq!(reason, DisconnectReason::ClientDisconnected);
    assert!(payload.is_none());
}

#[test]
fn server_disconnect_flag_maps_to_client_disconnected() {
    let (reason, payload) = Client::<u64>::resolve_disconnect_reason(None, false, true);
    assert_eq!(reason, DisconnectReason::ClientDisconnected);
    assert!(payload.is_none());
}

#[test]
fn bare_connection_drop_maps_to_timed_out() {
    let (reason, payload) = Client::<u64>::resolve_disconnect_reason(None, false, false);
    assert_eq!(reason, DisconnectReason::TimedOut);
    assert!(payload.is_none());
}

/// A scripted inbound transport: yields each packet once, then silence.
/// Lets established-state tests drive `maintain_socket` with exact wire
/// bytes, the way the server's `write_disconnect` emits them.
#[derive(Clone)]
struct ScriptedReceiver {
    packets: Vec<Vec<u8>>,
    next: usize,
}

impl PacketReceiver for ScriptedReceiver {
    fn receive(&mut self) -> Result<Option<&[u8]>, RecvError> {
        let out = self.packets.get(self.next).map(Vec::as_slice);
        if out.is_some() {
            self.next += 1;
        }
        Ok(out)
    }

    fn server_addr(&self) -> ServerAddr {
        dummy_server()
    }
}

/// Byte-identical to the server's `write_disconnect`: standard header +
/// `ServerDisconnect` header + optional message payload.
fn server_disconnect_packet(reason: DisconnectReason, payload: Option<&[u8]>) -> Vec<u8> {
    let mut writer = BitWriter::new();
    StandardHeader::new(PacketType::Handshake, 0, 0, 0).ser(&mut writer);
    HandshakeHeader::ServerDisconnect(reason).ser(&mut writer);
    payload.map(|bytes| bytes.to_vec()).ser(&mut writer);
    writer.to_packet().slice().to_vec()
}

fn connected_client_with_receiver(
    sender: Box<dyn PacketSender>,
    receiver: Box<dyn PacketReceiver>,
) -> Client<u64> {
    let mut client = loading_client_with_receiver(sender, receiver);
    client
        .handshake_manager
        .set_identity_token(IdentityToken::generate());
    let time_manager = TimeManager::from_parts(
        Duration::from_millis(100),
        BaseTimeManager::new(),
        0,
        GameInstant::new(&Instant::now()),
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
    );
    client.server_connection = Some(Connection::new(
        &client.client_config.connection,
        &client.protocol.channel_kinds,
        time_manager,
        &client.global_world_manager,
        client.client_config.jitter_buffer,
        &client.protocol.component_kinds,
    ));
    client
}

#[test]
fn repeated_server_disconnect_keeps_first_reason_and_payload() {
    // The server sends the disconnect several times for reliability; the
    // client must report the FIRST reading, not the last — otherwise a
    // terminal Kicked could be overwritten by a later packet and the
    // consumer would auto-reconnect into an eviction loop.
    let sent = Arc::new(AtomicUsize::new(0));
    let mut client = connected_client_with_receiver(
        Box::new(CountingSender { sent }),
        Box::new(ScriptedReceiver {
            packets: vec![
                server_disconnect_packet(DisconnectReason::Kicked, Some(&[1, 2, 3])),
                server_disconnect_packet(DisconnectReason::TimedOut, Some(&[9])),
            ],
            next: 0,
        }),
    );
    client.maintain_socket();
    assert!(client.server_disconnect);
    assert_eq!(
        client.server_disconnect_details,
        Some((DisconnectReason::Kicked, Some(vec![1u8, 2, 3]))),
        "a repeated server disconnect must not clobber the first reason",
    );
}

/// A Ping whose ping_index is truncated to one byte (`PingIndex` is a
/// `u16`): undecodable at the packet boundary, the way a malformed
/// server Ping arrives.
fn truncated_ping_packet() -> Vec<u8> {
    let mut writer = BitWriter::new();
    StandardHeader::new(PacketType::Ping, 0, 0, 0).ser(&mut writer);
    0u16.ser(&mut writer);
    GameInstant::new(&Instant::now()).ser(&mut writer);
    0xABu8.ser(&mut writer);
    writer.to_packet().slice().to_vec()
}

#[test]
fn truncated_ping_index_is_dropped_without_panic() {
    // common-02: remote input must not panic. A Ping with an undecodable
    // ping_index is dropped like any other malformed inbound; the live
    // connection is untouched and no pong is owed for bytes unheard.
    let sent = Arc::new(AtomicUsize::new(0));
    let mut client = connected_client_with_receiver(
        Box::new(CountingSender { sent: sent.clone() }),
        Box::new(ScriptedReceiver {
            packets: vec![truncated_ping_packet()],
            next: 0,
        }),
    );
    client.maintain_socket();
    assert_eq!(
        client.connection_status(),
        ConnectionStatus::Connected,
        "a malformed Ping must not disturb the live connection",
    );
    assert!(
        !client.server_disconnect,
        "a malformed Ping is not a server disconnect",
    );
    assert_eq!(
        sent.load(Ordering::SeqCst),
        0,
        "no pong is owed for a ping_index never decoded",
    );
}

/// A bare keep-alive, the way the server's heartbeat task emits it.
#[cfg(feature = "test_time")]
fn heartbeat_packet() -> Vec<u8> {
    let mut writer = BitWriter::new();
    StandardHeader::new(PacketType::Heartbeat, 0, 0, 0).ser(&mut writer);
    writer.to_packet().slice().to_vec()
}

#[cfg(feature = "test_time")]
#[test]
fn silence_past_timeout_marks_connection_for_drop() {
    // 30 s default timeout: a peer gone silent that long must read as
    // disconnecting, resolving to TimedOut downstream (safe to
    // auto-reconnect). Each test runs on its own thread so the
    // thread-local TestClock is clean.
    std::thread::spawn(|| {
        TestClock::init(0);
        let sent = Arc::new(AtomicUsize::new(0));
        let mut client = connected_client_with_receiver(
            Box::new(CountingSender { sent }),
            Box::new(ScriptedReceiver {
                packets: vec![],
                next: 0,
            }),
        );
        assert!(!client.is_disconnecting());
        TestClock::advance(30_001);
        client.maintain_socket();
        assert!(
            client.is_disconnecting(),
            "30 s of silence must mark the connection for drop",
        );
    })
    .join()
    .unwrap();
}

#[cfg(feature = "test_time")]
#[test]
fn heard_packet_rearms_drop_timer() {
    // Any heard packet — even a bare heartbeat — resets the timeout:
    // 29 s + heartbeat + 29 s must NOT read as disconnecting, but a
    // further 2 s of silence must.
    std::thread::spawn(|| {
        TestClock::init(0);
        let sent = Arc::new(AtomicUsize::new(0));
        let mut client = connected_client_with_receiver(
            Box::new(CountingSender { sent }),
            Box::new(ScriptedReceiver {
                packets: vec![heartbeat_packet()],
                next: 0,
            }),
        );
        TestClock::advance(29_000);
        client.maintain_socket();
        assert!(!client.is_disconnecting());
        TestClock::advance(29_000);
        client.maintain_socket();
        assert!(
            !client.is_disconnecting(),
            "a heartbeat 29 s ago must still hold the connection",
        );
        TestClock::advance(2_000);
        client.maintain_socket();
        assert!(
            client.is_disconnecting(),
            "31 s after the last heard packet must mark drop",
        );
    })
    .join()
    .unwrap();
}

/// A sender that keeps every payload, so keep-alive tests can inspect
/// what the client actually emitted while silent.
#[cfg(feature = "test_time")]
#[derive(Clone)]
struct CapturingSender {
    sent: Arc<std::sync::Mutex<Vec<Vec<u8>>>>,
}

#[cfg(feature = "test_time")]
impl PacketSender for CapturingSender {
    fn send(&self, payload: &[u8]) -> Result<(), SendError> {
        self.sent.lock().unwrap().push(payload.to_vec());
        Ok(())
    }

    fn server_addr(&self) -> ServerAddr {
        dummy_server()
    }

    fn shutdown(&mut self) {}
}

#[cfg(feature = "test_time")]
#[test]
fn silent_client_emits_keep_alive_past_interval() {
    // 4 s default heartbeat interval: the silent side must speak first,
    // or the peer's 30 s drop timer fires on a merely-idle connection
    // and a healthy link flaps through TimedOut reconnects. The client
    // keeps alive with Ping (latency probe) and Heartbeat packets —
    // never Data.
    std::thread::spawn(|| {
        TestClock::init(0);
        let sent = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut client = connected_client_with_receiver(
            Box::new(CapturingSender { sent: sent.clone() }),
            Box::new(ScriptedReceiver {
                packets: vec![],
                next: 0,
            }),
        );
        // Idle past the heartbeat interval: the client must speak.
        TestClock::advance(4_001);
        client.maintain_socket();
        let sent = sent.lock().unwrap();
        assert!(
            !sent.is_empty(),
            "a silent client must emit a keep-alive past the heartbeat interval",
        );
        for payload in sent.iter() {
            let mut reader = BitReader::new(payload);
            let header = StandardHeader::de(&mut reader).expect("keep-alive must parse");
            assert!(
                matches!(header.packet_type, PacketType::Ping | PacketType::Heartbeat),
                "idle emissions must be keep-alives, not data: {:?}",
                header.packet_type,
            );
        }
    })
    .join()
    .unwrap();
}

/// Drain the public disconnection events, so give-up tests can assert
/// exact event counts and reasons.
#[cfg(feature = "test_time")]
fn take_disconnects(
    client: &mut Client<u64>,
) -> Vec<(SocketAddr, DisconnectReason, Option<MessageContainer>)> {
    DisconnectEvent::iter(&mut client.take_world_events()).collect()
}

#[cfg(feature = "test_time")]
#[test]
fn silent_handshake_gives_up_with_auth_timeout_at_deadline() {
    // A server that never answers must surface exactly one AuthTimeout
    // at the 30 s link-silence deadline — never before, never twice —
    // so the consumer's offline path learns the attempt failed instead
    // of watching "connecting" forever.
    std::thread::spawn(|| {
        TestClock::init(0);
        let sent = Arc::new(AtomicUsize::new(0));
        let mut client = loading_client(CountingSender { sent });
        assert_eq!(client.connection_status(), ConnectionStatus::Connecting);

        // Just inside the deadline: still trying, no event.
        TestClock::advance(29_999);
        client.maintain_socket();
        assert_eq!(client.connection_status(), ConnectionStatus::Connecting);
        assert!(take_disconnects(&mut client).is_empty());

        // Past it: exactly one AuthTimeout carrying the dial target.
        TestClock::advance(2);
        client.maintain_socket();
        assert_eq!(client.connection_status(), ConnectionStatus::Disconnected);
        let events = take_disconnects(&mut client);
        assert_eq!(events.len(), 1, "give-up must emit exactly one event");
        assert_eq!(events[0].0, "127.0.0.1:9999".parse().unwrap());
        assert_eq!(events[0].1, DisconnectReason::AuthTimeout);
        assert!(events[0].2.is_none());

        // A further tick emits nothing more: the reset disarmed the timer.
        client.maintain_socket();
        assert!(take_disconnects(&mut client).is_empty());
    })
    .join()
    .unwrap();
}

/// A receiver that delivers exactly one undecodable packet, then
/// silence: the peer is alive (bytes arrive) but says nothing the
/// handshake can use. Proves heard traffic re-arms the give-up timer.
#[cfg(feature = "test_time")]
#[derive(Clone)]
struct OnceReceiver {
    packet: Vec<u8>,
    live: bool,
}

#[cfg(feature = "test_time")]
impl PacketReceiver for OnceReceiver {
    fn receive(&mut self) -> Result<Option<&[u8]>, RecvError> {
        if self.live {
            self.live = false;
            Ok(Some(self.packet.as_slice()))
        } else {
            Ok(None)
        }
    }

    fn server_addr(&self) -> ServerAddr {
        dummy_server()
    }
}

#[cfg(feature = "test_time")]
#[test]
fn heard_handshake_traffic_rearms_give_up_timer() {
    // The give-up deadline measures link SILENCE, not time since dial:
    // a peer that keeps sending — even packets the handshake cannot
    // use — keeps the attempt alive. Without the re-arm, a
    // healthy-but-slow handshake under a short silence window reports
    // AuthTimeout mid-connect and kills the attempt (heartbeat-timeout
    // e2e regression: first-bad e567e1fe).
    std::thread::spawn(|| {
        TestClock::init(0);
        let sent = Arc::new(AtomicUsize::new(0));
        let mut client = loading_client_with_receiver(
            Box::new(CountingSender { sent }),
            Box::new(OnceReceiver {
                packet: vec![0xFF; 8],
                live: true,
            }),
        );
        assert_eq!(client.connection_status(), ConnectionStatus::Connecting);

        // Near the deadline a packet arrives: still trying, no event.
        TestClock::advance(29_999);
        client.maintain_socket();
        assert_eq!(client.connection_status(), ConnectionStatus::Connecting);
        assert!(take_disconnects(&mut client).is_empty());

        // Past the ORIGINAL deadline the attempt survives: the heard
        // packet re-armed the timer.
        TestClock::advance(2);
        client.maintain_socket();
        assert_eq!(client.connection_status(), ConnectionStatus::Connecting);
        assert!(take_disconnects(&mut client).is_empty());

        // A full silent window after the last heard packet still gives
        // up: exactly one AuthTimeout, attempt over.
        TestClock::advance(30_000);
        client.maintain_socket();
        assert_eq!(client.connection_status(), ConnectionStatus::Disconnected);
        let events = take_disconnects(&mut client);
        assert_eq!(events.len(), 1, "silence must still give up exactly once");
        assert_eq!(events[0].1, DisconnectReason::AuthTimeout);
    })
    .join()
    .unwrap();
}

/// A sender whose transport never learned the peer address: the dial
/// target never resolved to a socket address.
#[cfg(feature = "test_time")]
#[derive(Clone)]
struct FindingSender;

#[cfg(feature = "test_time")]
impl PacketSender for FindingSender {
    fn send(&self, _payload: &[u8]) -> Result<(), SendError> {
        Ok(())
    }

    fn server_addr(&self) -> ServerAddr {
        ServerAddr::Finding
    }

    fn shutdown(&mut self) {}
}

#[cfg(feature = "test_time")]
#[test]
fn give_up_with_unknown_peer_emits_no_event_but_resets() {
    // Deadline with the peer address never learned: there is nothing
    // honest to attribute an AuthTimeout to, so no event is
    // fabricated — but the attempt still tears down to Disconnected
    // so the consumer's retry loop re-engages instead of watching
    // "connecting" forever.
    std::thread::spawn(|| {
        TestClock::init(0);
        let mut client =
            loading_client_with_receiver(Box::new(FindingSender), Box::new(EmptyReceiver));
        assert_eq!(client.connection_status(), ConnectionStatus::Connecting);

        TestClock::advance(30_001);
        client.maintain_socket();
        assert_eq!(client.connection_status(), ConnectionStatus::Disconnected);
        assert!(
            take_disconnects(&mut client).is_empty(),
            "no address means no event, never a manufactured one"
        );
    })
    .join()
    .unwrap();
}

#[cfg(feature = "test_time")]
#[test]
fn established_connection_ignores_handshake_deadline() {
    // The give-up timer is a handshake-only instrument: a live
    // connection kept awake by heartbeats must never consult it, no
    // matter how long the attempt clock has been running.
    std::thread::spawn(|| {
        TestClock::init(0);
        let sent = Arc::new(AtomicUsize::new(0));
        let mut client = connected_client_with_receiver(
            Box::new(CountingSender { sent }),
            Box::new(ScriptedReceiver {
                packets: vec![
                    heartbeat_packet(),
                    heartbeat_packet(),
                    heartbeat_packet(),
                    heartbeat_packet(),
                ],
                next: 0,
            }),
        );
        // 60 s total — twice the deadline — with a heartbeat every 15 s.
        for _ in 0..4 {
            TestClock::advance(15_000);
            client.maintain_socket();
        }
        assert!(client.server_connection.is_some());
        assert!(
            take_disconnects(&mut client).is_empty(),
            "an established connection must never report AuthTimeout",
        );
    })
    .join()
    .unwrap();
}

#[cfg(feature = "test_time")]
#[test]
fn cancel_before_deadline_gives_no_auth_timeout() {
    // Abandoning the attempt wins over the deadline: no event for an
    // attempt the consumer already withdrew.
    std::thread::spawn(|| {
        TestClock::init(0);
        let sent = Arc::new(AtomicUsize::new(0));
        let mut client = loading_client(CountingSender { sent });
        TestClock::advance(29_999);
        client.maintain_socket();
        client.cancel_connect();
        TestClock::advance(60_000);
        client.maintain_socket();
        client.maintain_socket();
        assert_eq!(client.connection_status(), ConnectionStatus::Disconnected);
        assert!(take_disconnects(&mut client).is_empty());
    })
    .join()
    .unwrap();
}

#[test]
fn cancel_connect_drops_poisoned_server_disconnect_flags() {
    // DWO-2: a stale explicit-disconnect flag (e.g. left by a previous
    // incarnation of this client object) must not survive into the next
    // attempt — the first post-handshake tick would otherwise take the
    // disconnect path again with a wrong ClientDisconnected reason.
    let sent = Arc::new(AtomicUsize::new(0));
    let mut client = loading_client(CountingSender { sent: sent.clone() });
    assert_eq!(client.connection_status(), ConnectionStatus::Connecting);
    client.server_disconnect = true;
    client.server_disconnect_details = Some((DisconnectReason::Kicked, Some(vec![1u8])));
    client.cancel_connect();
    assert_eq!(client.connection_status(), ConnectionStatus::Disconnected);
    assert!(
        !client.server_disconnect && client.server_disconnect_details.is_none(),
        "cancel must drop stale server-disconnect state with the attempt",
    );
}

/// One dial attempt's observable transport (Drake 42499 s1). `open` is
/// the peer: shutdown must drive it dark. `order` records dials and
/// shutdowns across attempts on one shared wire timeline.
#[derive(Clone)]
struct AttemptWire {
    order: Arc<Mutex<Vec<&'static str>>>,
}

#[derive(Clone)]
struct AttemptSender {
    wire: AttemptWire,
    open: Arc<AtomicBool>,
    shutdown_tag: &'static str,
}

impl AttemptWire {
    fn record(&self, event: &'static str) {
        self.order.lock().unwrap().push(event);
    }
}

impl PacketSender for AttemptSender {
    fn send(&self, _payload: &[u8]) -> Result<(), SendError> {
        if self.open.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(SendError)
        }
    }

    fn server_addr(&self) -> ServerAddr {
        dummy_server()
    }

    fn shutdown(&mut self) {
        // The effect, not a hook record: the peer goes dark, so a send
        // after shutdown fails. First call wins; repeats change nothing.
        if self.open.swap(false, Ordering::SeqCst) {
            self.wire.record(self.shutdown_tag);
        }
    }
}

/// A dial that mints a fresh attempt on one shared wire timeline.
struct DialSocket {
    wire: AttemptWire,
    dial_tag: &'static str,
    shutdown_tag: &'static str,
    attempt_open: Arc<AtomicBool>,
}

impl From<DialSocket> for Box<dyn Socket> {
    fn from(val: DialSocket) -> Self {
        Box::new(val)
    }
}

impl Socket for DialSocket {
    fn connect(
        self: Box<Self>,
        _protocol_id: naia_shared::ProtocolId,
    ) -> (
        Box<dyn IdentityReceiver>,
        Box<dyn PacketSender>,
        Box<dyn PacketReceiver>,
    ) {
        self.wire.record(self.dial_tag);
        (
            Box::new(OkIdReceiver),
            Box::new(AttemptSender {
                wire: AttemptWire {
                    order: self.wire.order.clone(),
                },
                open: self.attempt_open.clone(),
                shutdown_tag: self.shutdown_tag,
            }),
            Box::new(EmptyReceiver),
        )
    }

    fn connect_with_auth(
        self: Box<Self>,
        protocol_id: naia_shared::ProtocolId,
        _auth_bytes: Vec<u8>,
    ) -> (
        Box<dyn IdentityReceiver>,
        Box<dyn PacketSender>,
        Box<dyn PacketReceiver>,
    ) {
        self.connect(protocol_id)
    }

    fn connect_with_auth_headers(
        self: Box<Self>,
        protocol_id: naia_shared::ProtocolId,
        _auth_headers: Vec<(String, String)>,
    ) -> (
        Box<dyn IdentityReceiver>,
        Box<dyn PacketSender>,
        Box<dyn PacketReceiver>,
    ) {
        self.connect(protocol_id)
    }

    fn connect_with_auth_and_headers(
        self: Box<Self>,
        protocol_id: naia_shared::ProtocolId,
        _auth_bytes: Vec<u8>,
        _auth_headers: Vec<(String, String)>,
    ) -> (
        Box<dyn IdentityReceiver>,
        Box<dyn PacketSender>,
        Box<dyn PacketReceiver>,
    ) {
        self.connect(protocol_id)
    }
}

#[test]
fn retry_shuts_down_previous_attempt_before_redial() {
    // s1 (Drake 42499): abandoning an attempt and dialing again must tear
    // down the previous attempt's transport first. Otherwise the stale
    // peer stays live and a second connection shares the wire mid-run.
    let order = Arc::new(Mutex::new(Vec::new()));
    let wire = AttemptWire {
        order: order.clone(),
    };
    let a1_open = Arc::new(AtomicBool::new(true));
    let mut client = idle_client();
    client.connect(DialSocket {
        wire: AttemptWire {
            order: order.clone(),
        },
        dial_tag: "dial-a1",
        shutdown_tag: "shutdown-a1",
        attempt_open: a1_open.clone(),
    });
    assert_eq!(client.connection_status(), ConnectionStatus::Connecting);
    client.cancel_connect();
    // Idempotent across repeated resets: a second cancel on the now-idle
    // client meets an empty Io and must not re-fire the teardown.
    client.cancel_connect();
    assert_eq!(client.connection_status(), ConnectionStatus::Disconnected);
    client.connect(DialSocket {
        wire,
        dial_tag: "dial-a2",
        shutdown_tag: "shutdown-a2",
        attempt_open: Arc::new(AtomicBool::new(true)),
    });
    // EFFECT: the first attempt's transport is dark — a send on it now
    // fails — and it went dark before the second attempt dialed.
    assert!(
        !a1_open.load(Ordering::SeqCst),
        "abandoned attempt's transport must be shut down on retry",
    );
    assert_eq!(
        *order.lock().unwrap(),
        vec!["dial-a1", "shutdown-a1", "dial-a2"],
        "shutdown must fire exactly once, before the redial",
    );
}
