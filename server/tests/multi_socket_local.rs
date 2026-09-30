//! naia-lib/naia#207: one naia `Server` must be able to serve clients
//! arriving on two different transports at once, through a single `Socket`
//! handle (`MultiSocket` fan-in, `server.listen(multi)` unchanged).

#![cfg(feature = "transport_local")]

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use naia_server::transport::{
    local::{LocalServerSocket, LocalTransportHub, Socket as LocalSocket},
    multi::MultiSocket,
    PacketReceiver, Socket,
};
use naia_shared::ProtocolId;

fn recv_from(receiver: &mut Box<dyn PacketReceiver>, want_addr: SocketAddr, want_payload: &[u8]) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match receiver.receive().expect("receiver error") {
            Some((addr, bytes)) => {
                assert_eq!(addr, want_addr, "packet arrived from the wrong client");
                assert_eq!(bytes, want_payload, "payload corrupted through fan-in");
                return;
            }
            None => {
                assert!(
                    Instant::now() < deadline,
                    "timed out waiting for packet from {want_addr}"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

#[test]
fn multi_socket_serves_two_transports_through_one_handle() {
    let hub_a = LocalTransportHub::new("127.0.0.1:15001".parse().unwrap());
    let hub_b = LocalTransportHub::new("127.0.0.1:15002".parse().unwrap());

    // Client addresses are per-hub sequences (both hubs start at :12346),
    // so burn hub_b's first address to keep the two populations disjoint —
    // production transports never share a client address.
    let (addr_a, _, _, tx_a, rx_a) = hub_a.register_client();
    let _ = hub_b.register_client();
    let (addr_b, _, _, tx_b, rx_b) = hub_b.register_client();
    assert_ne!(addr_a, addr_b);

    let sock_a = LocalSocket::new(LocalServerSocket::new(hub_a), None);
    let sock_b = LocalSocket::new(LocalServerSocket::new(hub_b), None);
    let boxed: Box<dyn Socket> = MultiSocket::new(vec![Box::new(sock_a), Box::new(sock_b)]).into();
    let (_auth_sender, _auth_receiver, packet_sender, mut packet_receiver) =
        boxed.listen(ProtocolId::new(0x207));

    // A packet from client A arrives with A's address ...
    tx_a.send(vec![10, 20]).unwrap();
    recv_from(&mut packet_receiver, addr_a, &[10, 20]);
    // ... and B's packet arrives with B's address — each exactly once.
    tx_b.send(vec![30, 40]).unwrap();
    recv_from(&mut packet_receiver, addr_b, &[30, 40]);
    assert!(
        packet_receiver.receive().expect("receiver error").is_none(),
        "duplicate delivery through fan-in"
    );

    // Replies route back to the owning hub's client.
    packet_sender.send(&addr_a, &[50]).unwrap();
    assert_eq!(rx_a.recv_timeout(Duration::from_secs(5)).unwrap(), vec![50]);
    packet_sender.send(&addr_b, &[60]).unwrap();
    assert_eq!(rx_b.recv_timeout(Duration::from_secs(5)).unwrap(), vec![60]);
}

/// Sends route by the address's ORIGIN, never by guess: each reply arrives
/// ONLY on its own hub, and a send to an address no inner ever saw is an
/// error (#207 rework: first-Ok-wins put every reply on inner 0, because a
/// real UDP/WebRTC send to an unseen address usually still returns Ok).
///
/// NOTE: local hub sends are strict (Err on unseen addr), so first-Ok looks
/// correct here by accident — except when both hubs know the address. Hub
/// client addresses are per-hub sequences, so client C on hub A shares B's
/// address; only origin routing tells them apart.
#[test]
fn multi_socket_replies_reach_only_the_origin_hub() {
    let hub_a = LocalTransportHub::new("127.0.0.1:15101".parse().unwrap());
    let hub_b = LocalTransportHub::new("127.0.0.1:15102".parse().unwrap());

    let (addr_a, _, _, tx_a, rx_a) = hub_a.register_client();
    let (_addr_c, _, _, _tx_c, rx_c) = hub_a.register_client();
    let _ = hub_b.register_client();
    let (addr_b, _, _, tx_b, rx_b) = hub_b.register_client();
    // The setup that breaks guessing: C (hub A) shares B's (hub B) address.
    assert_eq!(_addr_c, addr_b);
    assert_ne!(addr_a, addr_b);

    let sock_a = LocalSocket::new(LocalServerSocket::new(hub_a), None);
    let sock_b = LocalSocket::new(LocalServerSocket::new(hub_b), None);
    let boxed: Box<dyn Socket> = MultiSocket::new(vec![Box::new(sock_a), Box::new(sock_b)]).into();
    let (_auth_sender, _auth_receiver, packet_sender, mut packet_receiver) =
        boxed.listen(ProtocolId::new(0x207));

    // A and B knock, so their origins are recorded. C never knocks.
    tx_a.send(vec![1]).unwrap();
    recv_from(&mut packet_receiver, addr_a, &[1]);
    tx_b.send(vec![2]).unwrap();
    recv_from(&mut packet_receiver, addr_b, &[2]);

    // B's reply must reach B — and stay off C's inbox on the other hub,
    // even though C owns the same address there. First-Ok-wins delivers
    // to inner 0 (C) instead.
    packet_sender.send(&addr_b, &[20]).unwrap();
    assert_eq!(rx_b.recv_timeout(Duration::from_secs(5)).unwrap(), vec![20]);
    assert!(
        rx_c.try_recv().is_err(),
        "B's reply leaked onto A's transport (same address, wrong origin)"
    );

    // A's reply is unaffected.
    packet_sender.send(&addr_a, &[10]).unwrap();
    assert_eq!(rx_a.recv_timeout(Duration::from_secs(5)).unwrap(), vec![10]);

    // Unknown address: error, not a guess at inner 0.
    let unknown: SocketAddr = "127.0.0.1:19999".parse().unwrap();
    assert!(
        packet_sender.send(&unknown, &[30]).is_err(),
        "send to an unseen address must fail"
    );
}

/// Receives are fair: polling rotates across inners, so a busy inner 0
/// cannot starve inner 1 (#207 rework: fixed-order first-Some-wins would
/// drain all of inner 0 first).
#[test]
fn multi_socket_receive_is_round_robin() {
    let hub_a = LocalTransportHub::new("127.0.0.1:15201".parse().unwrap());
    let hub_b = LocalTransportHub::new("127.0.0.1:15202".parse().unwrap());

    let (addr_a, _, _, tx_a, _rx_a) = hub_a.register_client();
    let _ = hub_b.register_client();
    let (addr_b, _, _, tx_b, _rx_b) = hub_b.register_client();

    let sock_a = LocalSocket::new(LocalServerSocket::new(hub_a), None);
    let sock_b = LocalSocket::new(LocalServerSocket::new(hub_b), None);
    let boxed: Box<dyn Socket> = MultiSocket::new(vec![Box::new(sock_a), Box::new(sock_b)]).into();
    let (_auth_sender, _auth_receiver, _packet_sender, mut packet_receiver) =
        boxed.listen(ProtocolId::new(0x207));

    // Inner 0 preloaded with 5 packets, inner 1 with 1.
    for i in 0..5u8 {
        tx_a.send(vec![100 + i]).unwrap();
    }
    tx_b.send(vec![200]).unwrap();

    // Poll 1 serves inner 0's head; poll 2 MUST serve inner 1 — not inner
    // 0's second packet.
    recv_from(&mut packet_receiver, addr_a, &[100]);
    recv_from(&mut packet_receiver, addr_b, &[200]);
}

// ---- Accept-everything stub transport (models UDP: udp.rs send_to
// returns Ok for ANY address, so first-Ok-wins silently misroutes). ----

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use naia_server::transport::{AuthReceiver, AuthSender, PacketSender, RecvError, SendError};
use naia_shared::IdentityToken;

type SharedCalls = Arc<Mutex<Vec<(SocketAddr, Vec<u8>)>>>;
type SharedQueue = Arc<Mutex<VecDeque<(SocketAddr, Vec<u8>)>>>;

#[derive(Clone, Default)]
struct CallLog(SharedCalls);

#[derive(Clone, Default)]
struct StubSocket {
    sends: CallLog,
    accepts: CallLog,
    inbound_data: SharedQueue,
    inbound_auth: SharedQueue,
}

impl Socket for StubSocket {
    fn listen(
        self: Box<Self>,
        _expected_protocol_id: ProtocolId,
    ) -> naia_server::transport::ListenResult {
        let inner = self.as_ref().clone();
        (
            Box::new(StubAuthSender {
                inner: inner.clone(),
            }),
            Box::new(StubAuthReceiver {
                inner: inner.clone(),
                last: None,
            }),
            Box::new(StubPacketSender {
                inner: inner.clone(),
            }),
            Box::new(StubPacketReceiver { inner, last: None }),
        )
    }
}

#[derive(Clone)]
struct StubAuthSender {
    inner: StubSocket,
}

impl AuthSender for StubAuthSender {
    fn accept(
        &self,
        address: &SocketAddr,
        _identity_token: &IdentityToken,
    ) -> Result<(), SendError> {
        self.inner
            .accepts
            .0
            .lock()
            .unwrap()
            .push((*address, Vec::new()));
        Ok(())
    }

    fn reject(&self, address: &SocketAddr, _payload: Option<&[u8]>) -> Result<(), SendError> {
        self.inner
            .accepts
            .0
            .lock()
            .unwrap()
            .push((*address, Vec::new()));
        Ok(())
    }
}

#[derive(Clone)]
struct StubAuthReceiver {
    inner: StubSocket,
    last: Option<(SocketAddr, Box<[u8]>)>,
}

impl AuthReceiver for StubAuthReceiver {
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
        let next = self.inner.inbound_auth.lock().unwrap().pop_front();
        match next {
            Some((addr, bytes)) => {
                self.last = Some((addr, bytes.into_boxed_slice()));
                let (addr, payload) = self.last.as_ref().unwrap();
                Ok(Some((*addr, payload.as_ref())))
            }
            None => {
                self.last = None;
                Ok(None)
            }
        }
    }
}

#[derive(Clone)]
struct StubPacketSender {
    inner: StubSocket,
}

impl PacketSender for StubPacketSender {
    fn send(&self, address: &SocketAddr, payload: &[u8]) -> Result<(), SendError> {
        self.inner
            .sends
            .0
            .lock()
            .unwrap()
            .push((*address, payload.to_vec()));
        Ok(())
    }
}

#[derive(Clone)]
struct StubPacketReceiver {
    inner: StubSocket,
    last: Option<(SocketAddr, Box<[u8]>)>,
}

impl PacketReceiver for StubPacketReceiver {
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
        let next = self.inner.inbound_data.lock().unwrap().pop_front();
        match next {
            Some((addr, bytes)) => {
                self.last = Some((addr, bytes.into_boxed_slice()));
                let (addr, payload) = self.last.as_ref().unwrap();
                Ok(Some((*addr, payload.as_ref())))
            }
            None => {
                self.last = None;
                Ok(None)
            }
        }
    }
}

/// Card req 1: an accept-everything inner in position 0 (the UDP shape) must
/// never capture another transport's replies. First-Ok-wins fails this: the
/// stub returns Ok for anything, so every send lands in its log and the real
/// client times out.
#[test]
fn multi_socket_never_guesses_into_accepting_inner() {
    let stub = StubSocket::default();
    let stub_log = stub.sends.clone();

    let hub = LocalTransportHub::new("127.0.0.1:15301".parse().unwrap());
    let (addr_b, _, _, tx_b, rx_b) = hub.register_client();

    let sock_hub = LocalSocket::new(LocalServerSocket::new(hub), None);
    let boxed: Box<dyn Socket> = MultiSocket::new(vec![Box::new(stub), Box::new(sock_hub)]).into();
    let (_auth_sender, _auth_receiver, packet_sender, mut packet_receiver) =
        boxed.listen(ProtocolId::new(0x207));

    // The hub client knocks; its origin (inner 1) is recorded.
    tx_b.send(vec![7]).unwrap();
    recv_from(&mut packet_receiver, addr_b, &[7]);

    // The reply must reach the hub client with ZERO sends into the stub.
    packet_sender.send(&addr_b, &[8]).unwrap();
    assert_eq!(rx_b.recv_timeout(Duration::from_secs(5)).unwrap(), vec![8]);
    assert!(
        stub_log.0.lock().unwrap().is_empty(),
        "reply to a hub client leaked into the accept-everything inner"
    );

    // Unknown address: error, not a guess at inner 0.
    let unknown: SocketAddr = "127.0.0.1:19999".parse().unwrap();
    assert!(packet_sender.send(&unknown, &[9]).is_err());
}

/// Auth plane, same rule: accept/reject go through the recorded origin.
/// Scripted inbound on inner 1; inner 0 must record no accepts.
#[test]
fn multi_socket_auth_follows_recorded_origin() {
    let stub0 = StubSocket::default();
    let stub1 = StubSocket::default();
    let log0 = stub0.accepts.clone();
    let log1 = stub1.accepts.clone();

    let addr_q: SocketAddr = "127.0.0.1:15401".parse().unwrap();
    stub1
        .inbound_auth
        .lock()
        .unwrap()
        .push_back((addr_q, vec![1, 2, 3]));

    let boxed: Box<dyn Socket> = MultiSocket::new(vec![Box::new(stub0), Box::new(stub1)]).into();
    let (auth_sender, mut auth_receiver, _packet_sender, _packet_receiver) =
        boxed.listen(ProtocolId::new(0x207));

    // Drain until the scripted request arrives (records origin 1).
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match auth_receiver.receive().expect("auth receiver error") {
            Some((addr, _)) if addr == addr_q => break,
            Some(_) => continue,
            None => {
                assert!(
                    Instant::now() < deadline,
                    "timed out waiting for scripted auth request"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    auth_sender
        .accept(&addr_q, &IdentityToken::generate())
        .unwrap();
    assert!(
        log0.0.lock().unwrap().is_empty(),
        "accept leaked into inner 0 (not the recorded origin)"
    );
    assert_eq!(log1.0.lock().unwrap().len(), 1);
    assert_eq!(log1.0.lock().unwrap()[0].0, addr_q);
}

// ---- [b4-oom] failing-inner transport: every receive fails persistently
// (models a disconnected channel at teardown). ----

#[derive(Clone, Default)]
struct FailSocket;

impl Socket for FailSocket {
    fn listen(
        self: Box<Self>,
        _expected_protocol_id: ProtocolId,
    ) -> naia_server::transport::ListenResult {
        (
            Box::new(StubAuthSender {
                inner: StubSocket::default(),
            }),
            Box::new(FailAuthReceiver),
            Box::new(StubPacketSender {
                inner: StubSocket::default(),
            }),
            Box::new(FailPacketReceiver),
        )
    }
}

#[derive(Clone)]
struct FailAuthReceiver;

impl AuthReceiver for FailAuthReceiver {
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
        Err(RecvError)
    }
}

#[derive(Clone)]
struct FailPacketReceiver;

impl PacketReceiver for FailPacketReceiver {
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
        Err(RecvError)
    }
}

/// [b4-oom] A failing inner must not swallow packets queued behind it: skip
/// the failure, keep draining, and surface the error only once the line is
/// quiet. Pre-fix the `?` on the failing inner propagates immediately and
/// the drained-behind packet in `last_payload` is never delivered.
#[test]
fn multi_packet_receiver_skips_failing_inner_without_losing_packets() {
    let stub = StubSocket::default();
    let addr: SocketAddr = "127.0.0.1:15501".parse().unwrap();
    stub.inbound_data
        .lock()
        .unwrap()
        .push_back((addr, vec![7, 7]));

    let boxed: Box<dyn Socket> =
        MultiSocket::new(vec![Box::new(FailSocket), Box::new(stub)]).into();
    let (_, _, _, mut receiver) = boxed.listen(ProtocolId::new(0x207));

    match receiver.receive() {
        Ok(Some((a, p))) => {
            assert_eq!(a, addr, "packet came from the wrong client");
            assert_eq!(p, &[7, 7], "payload corrupted behind failing inner");
        }
        other => panic!("packet queued behind a failing inner was lost: {other:?}"),
    }
    // Nothing left but the persistent failure: it must still surface.
    assert!(
        receiver.receive().is_err(),
        "quiet failing inner must still report its error"
    );
}

/// 12121: forgetting an address evicts its origin entry — a later send to
/// it is an error again, exactly as if it had never knocked. The defaulted
/// no-op keeps every non-multi `PacketSender` compiling and behaving
/// unchanged (the stub still routes after `forget_address`).
#[test]
fn packet_sender_forget_evicts_origin() {
    let stub = StubSocket::default();
    let addr: SocketAddr = "127.0.0.1:15601".parse().unwrap();
    stub.inbound_data.lock().unwrap().push_back((addr, vec![1]));

    let boxed: Box<dyn Socket> = MultiSocket::new(vec![Box::new(stub.clone())]).into();
    let (_, _, packet_sender, mut packet_receiver) = boxed.listen(ProtocolId::new(0x207));

    // Knock records the origin; the reply routes.
    let next = packet_receiver.receive().expect("receiver error");
    assert!(matches!(next, Some((a, _)) if a == addr));
    packet_sender.send(&addr, &[2]).unwrap();

    // Forget evicts: the address is unknown again.
    packet_sender.forget_address(&addr);
    assert!(
        packet_sender.send(&addr, &[3]).is_err(),
        "send to a forgotten address must fail"
    );

    // The defaulted no-op: a non-multi sender is unaffected by forget.
    let plain = StubPacketSender {
        inner: stub.clone(),
    };
    plain.forget_address(&addr);
    plain.send(&addr, &[4]).unwrap();
}

/// 12121, auth plane: a rejected knock's origin must be evictable too —
/// `reject` alone routes through the entry but leaves it behind.
#[test]
fn auth_sender_forget_evicts_rejected_origin() {
    let stub = StubSocket::default();
    let addr: SocketAddr = "127.0.0.1:15602".parse().unwrap();
    stub.inbound_auth.lock().unwrap().push_back((addr, vec![5]));

    let boxed: Box<dyn Socket> = MultiSocket::new(vec![Box::new(stub)]).into();
    let (auth_sender, mut auth_receiver, packet_sender, _) = boxed.listen(ProtocolId::new(0x207));

    // Knock records the origin; the reject routes through it.
    let next = auth_receiver.receive().expect("auth receiver error");
    assert!(matches!(next, Some((a, _)) if a == addr));
    auth_sender
        .reject(&addr, None)
        .expect("reject to a known origin must succeed");

    // ... but the entry lingers: forgetting is what removes it.
    packet_sender.send(&addr, &[6]).unwrap();
    auth_sender.forget_address(&addr);
    assert!(
        packet_sender.send(&addr, &[7]).is_err(),
        "rejected address must refuse sends after forget"
    );
}

/// [b4-oom] Auth plane, same rule: a knock queued behind a failing inner
/// must still arrive, and the persistent error must still surface.
#[test]
fn multi_auth_receiver_skips_failing_inner_without_losing_knocks() {
    let stub = StubSocket::default();
    let addr: SocketAddr = "127.0.0.1:15502".parse().unwrap();
    stub.inbound_auth
        .lock()
        .unwrap()
        .push_back((addr, vec![9, 9]));

    let boxed: Box<dyn Socket> =
        MultiSocket::new(vec![Box::new(FailSocket), Box::new(stub)]).into();
    let (_, mut receiver, _, _) = boxed.listen(ProtocolId::new(0x207));

    match receiver.receive() {
        Ok(Some((a, p))) => {
            assert_eq!(a, addr, "knock came from the wrong client");
            assert_eq!(p, &[9, 9], "knock corrupted behind failing inner");
        }
        other => panic!("knock queued behind a failing inner was lost: {other:?}"),
    }
    assert!(
        receiver.receive().is_err(),
        "quiet failing inner must still report its error"
    );
}
