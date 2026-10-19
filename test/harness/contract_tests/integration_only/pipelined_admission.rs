//! Admission through the real pipelined receive/drain bracket, with the
//! production local transport and demo world. Registration must precede the
//! coordinator drain; a connect response alone does not prove finalization.

use std::{
    net::SocketAddr,
    sync::mpsc::{Receiver, Sender},
};

use naia_demo_world::{Entity, World};
use naia_server::{
    transport::local::{LocalServerSocket, LocalTransportHub, Socket},
    ConnectEvent, ErrorEvent, PipelinedWorldServer, ServerConfig, UserKey,
};
use naia_shared::{
    handshake::HandshakeHeader, BigMapKey, BitReader, BitWriter, PacketType, Serde, StandardHeader,
};
use naia_test_harness::protocol;

struct Client {
    address: SocketAddr,
    outgoing: Sender<Vec<u8>>,
    incoming: Receiver<Vec<u8>>,
}

impl Client {
    fn request_connect(&self) {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::Handshake, 0, 0, 0).ser(&mut writer);
        HandshakeHeader::ClientConnectRequest.ser(&mut writer);
        self.outgoing
            .send(writer.to_packet().slice().to_vec())
            .unwrap();
    }

    fn take_connect_responses(&self) -> usize {
        self.incoming
            .try_iter()
            .filter(|bytes| {
                let mut reader = BitReader::new(bytes);
                let header = StandardHeader::de(&mut reader).unwrap();
                header.packet_type == PacketType::Handshake
                    && HandshakeHeader::de(&mut reader).unwrap()
                        == HandshakeHeader::ServerConnectResponse
            })
            .count()
    }
}

struct Admission {
    server: PipelinedWorldServer<Entity>,
    world: World,
    hub: LocalTransportHub,
}

impl Admission {
    fn new() -> Self {
        let hub = LocalTransportHub::new("127.0.0.1:54490".parse().unwrap());
        hub.enable_packet_recording();
        let mut server = PipelinedWorldServer::new(ServerConfig::default(), protocol());
        server.listen(Socket::new(LocalServerSocket::new(hub.clone()), None));
        Self {
            server,
            world: World::default(),
            hub,
        }
    }

    fn client(&self) -> Client {
        let (address, _auth_tx, _auth_rx, outgoing, incoming) = self.hub.register_client();
        Client {
            address,
            outgoing,
            incoming,
        }
    }

    fn receive(&mut self, expected_requests: usize) -> Vec<UserKey> {
        let mut outputs = Vec::new();
        self.server
            .receive_into(&mut self.world.proxy_mut(), &mut outputs);
        let mut connected = Vec::new();
        for output in &mut outputs {
            assert!(!output.world_events.has::<ErrorEvent>());
            connected.extend(output.world_events.read::<ConnectEvent>());
        }
        // apply_recv_to_world consumes output.received_addresses. Observe
        // receipt at the real transport instead of inspecting the drained set.
        let received: Vec<_> = self
            .hub
            .take_recorded_packets()
            .into_iter()
            .filter(|(server_to_client, _)| !server_to_client)
            .collect();
        assert_eq!(received.len(), expected_requests);
        for (_, bytes) in received {
            let mut reader = BitReader::new(&bytes);
            assert_eq!(
                StandardHeader::de(&mut reader).unwrap().packet_type,
                PacketType::Handshake
            );
            assert_eq!(
                HandshakeHeader::de(&mut reader).unwrap(),
                HandshakeHeader::ClientConnectRequest
            );
        }
        connected
    }

    fn assert_connection(&self, user_key: &UserKey, address: &SocketAddr, ready: bool) {
        assert_eq!(self.server.user_connection_ready(user_key), ready);
        let recv_slot = self.server.recv_slot();
        let recv = recv_slot.lock();
        assert_eq!(
            recv.as_ref()
                .unwrap()
                .state
                .recv_user_connections
                .contains_key(address),
            ready,
            "both connection halves must agree after the coordinator drain",
        );
    }

    fn send(&mut self) {
        self.server.send(&self.world.proxy());
    }
}

#[test]
fn registered_user_connects_through_real_receive_and_finalization() {
    let mut admission = Admission::new();
    let client = admission.client();
    let key = UserKey::from_u64(7);

    admission.server.receive_user(key, client.address);
    admission.assert_connection(&key, &client.address, false);
    client.request_connect();

    assert_eq!(admission.receive(1), vec![key]);
    admission.assert_connection(&key, &client.address, true);
    admission.send();
    assert_eq!(client.take_connect_responses(), 1);
    assert!(admission.receive(0).is_empty());
    admission.assert_connection(&key, &client.address, true);
}

#[test]
fn draining_before_registration_does_not_finalize_or_buffer_admission() {
    let mut admission = Admission::new();
    let client = admission.client();
    let key = UserKey::from_u64(7);

    client.request_connect();
    assert!(admission.receive(1).is_empty());
    admission.assert_connection(&key, &client.address, false);

    // The same operations in reversed order discriminate: registration after
    // the drain cannot resurrect a request that already lacked a user.
    admission.server.receive_user(key, client.address);
    assert!(admission.receive(0).is_empty());
    admission.assert_connection(&key, &client.address, false);
    admission.send();
    // Recv acknowledges even an unknown address. This is deliberately weaker
    // than the connection/event assertions above, and must not count as admission.
    assert_eq!(client.take_connect_responses(), 1);
}

#[test]
fn unknown_user_does_not_block_a_registered_user_in_the_same_drain() {
    let mut admission = Admission::new();
    let unknown = admission.client();
    let known = admission.client();
    let unknown_key = UserKey::from_u64(8);
    let known_key = UserKey::from_u64(7);

    admission.server.receive_user(known_key, known.address);
    // The local hub drains the lower-address unknown client first.
    unknown.request_connect();
    known.request_connect();
    assert_eq!(admission.receive(2), vec![known_key],);
    admission.assert_connection(&known_key, &known.address, true);
    admission.assert_connection(&unknown_key, &unknown.address, false);

    admission.server.receive_user(unknown_key, unknown.address);
    assert!(admission.receive(0).is_empty());
    admission.assert_connection(&unknown_key, &unknown.address, false);
    admission.send();
    assert_eq!(known.take_connect_responses(), 1);
    assert_eq!(unknown.take_connect_responses(), 1);
}

#[test]
fn duplicate_requests_finalize_once_across_receive_drains() {
    let mut admission = Admission::new();
    let client = admission.client();
    let key = UserKey::from_u64(7);

    admission.server.receive_user(key, client.address);
    client.request_connect();
    client.request_connect();
    assert_eq!(admission.receive(2), vec![key]);
    admission.assert_connection(&key, &client.address, true);
    admission.send();
    assert_eq!(client.take_connect_responses(), 2);

    client.request_connect();
    assert!(admission.receive(1).is_empty());
    admission.assert_connection(&key, &client.address, true);
    admission.send();
    assert_eq!(client.take_connect_responses(), 1);
}
