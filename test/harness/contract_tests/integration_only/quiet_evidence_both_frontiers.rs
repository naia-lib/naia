//! nydus-dm9: a real server-to-client quiet stretch must advance the
//! transmit and applied evidence frontiers together, with no gaps.
//!
//! The stream-level quiet test pins `CauseStream` in isolation. This test
//! drives a live local-transport link: after seeding one replicated entity,
//! nothing changes for forty ticks, and both sides' coverages must agree
//! tick-for-tick — the server's `transmit_coverage` frontier must equal the
//! client's `applied_coverage` frontier must equal the last received Data
//! tick, with empty gaps on both sides. Any divergence means the matcher
//! would flag a quiet link as broken.

use std::{sync::Arc, time::Duration};

use naia_client::{
    transport::local::{LocalAddrCell, LocalClientSocket, Socket as ClientSocket},
    Client, ClientConfig, JitterBufferType,
};
use naia_demo_world::{Entity, World};
use naia_server::{
    transport::local::{LocalServerSocket, LocalTransportHub, Socket as ServerSocket},
    AuthEvent, ReplicationConfig, Server, ServerConfig, ServerMode, UserKey,
};
use naia_shared::{Instant, TestClock, WorldRefType};
use naia_test_harness::{protocol, Auth, Position};
use parking_lot::Mutex;

struct Link {
    server: Server<Entity>,
    server_world: World,
    client: Client<Entity>,
    client_world: World,
    user: Option<UserKey>,
}

impl Link {
    #[expect(
        clippy::expect_used,
        reason = "test helper: listener failure must fail the calling test"
    )]
    fn new(mode: ServerMode) -> Self {
        TestClock::init(0);
        let hub = LocalTransportHub::new("127.0.0.1:54502".parse().unwrap());
        // Ada 184483: evidence is opt-in — this link asserts journals, so it
        // enables them on both sides.
        let server_config = ServerConfig {
            cause_evidence: true,
            ..ServerConfig::default()
        };
        let mut server = Server::new(mode, &server_config, protocol());
        server
            .listen(ServerSocket::new(LocalServerSocket::new(hub.clone()), None))
            .expect("server listener must bind");
        let (address, auth_tx, auth_rx, data_tx, data_rx) = hub.register_client();
        let addr_cell = LocalAddrCell::new();
        addr_cell.set_sync(hub.server_addr());
        let socket = LocalClientSocket::new_with_tokens(
            address,
            hub.server_addr(),
            auth_tx,
            auth_rx,
            data_tx,
            data_rx,
            addr_cell,
            Arc::new(Mutex::new(None)),
            Arc::new(Mutex::new(None)),
        );
        let client_config = ClientConfig {
            send_handshake_interval: Duration::ZERO,
            jitter_buffer: JitterBufferType::Bypass,
            cause_evidence: true,
            ..ClientConfig::default()
        };
        let mut client = Client::new(client_config, protocol());
        client.auth(Auth::new("quiet-link", "test"));
        client.connect(ClientSocket::new(socket, None));
        let mut link = Self {
            server,
            server_world: World::default(),
            client,
            client_world: World::default(),
            user: None,
        };
        for _ in 0..100 {
            link.step(true);
            if link.client.connection_status().is_connected()
                && link
                    .user
                    .is_some_and(|key| link.server.user_connection_ready(&key))
            {
                return link;
            }
        }
        panic!("the production local handshake must finish");
    }

    fn step(&mut self, deliver: bool) {
        TestClock::advance(50);
        let now = Instant::now();
        self.server.receive_all_packets();
        self.server
            .process_all_packets(self.server_world.proxy_mut(), &now);
        let mut events = self.server.take_world_events();
        for (key, _) in events.read::<AuthEvent<Auth>>() {
            self.server.accept_connection(&key);
            self.user = Some(key);
        }
        self.server.take_tick_events(&now);
        self.server.send_all_packets(&self.server_world.proxy());
        if deliver {
            self.client.receive_all_packets();
        }
        self.client.take_tick_events(&now);
        self.client
            .process_all_packets(self.client_world.proxy_mut(), &now);
        self.client.take_world_events();
        self.client.send_all_packets(self.client_world.proxy());
    }

    fn seed_replication(&mut self) {
        let room = self.server.create_room().key();
        self.server.room_mut(&room).add_user(&self.user.unwrap());
        let entity = self
            .server
            .spawn_entity(self.server_world.proxy_mut())
            .configure_replication(ReplicationConfig::public())
            .insert_component(Position::new(7.0, 8.0))
            .enter_room(&room)
            .id();
        self.server
            .user_scope_mut(&self.user.unwrap())
            .include(&entity);
        for _ in 0..80 {
            self.step(true);
        }
        assert_eq!(self.client_world.proxy().entities().len(), 1);
    }

    /// Both evidence frontiers advance on quiet ticks with no gaps and no
    /// refusals: the server frontier sits on its latest send tick, and the
    /// client frontier sits on its receiving tick — the confirmed timeline
    /// it has actually applied. Packets newer than the receiving tick wait
    /// in the jitter buffer (pop_item releases only tick <= receiving
    /// tick), so last_received may lead the client frontier by the
    /// de-jitter margin. That lag is the gate working, not a gap.
    fn assert_frontiers_agree(&self, step: usize) {
        let user = self.user.unwrap();
        let received = self
            .client
            .last_received_server_tick()
            .expect("client receives Data every quiet step");
        let server_cov = self
            .server
            .transmit_coverage(&user)
            .expect("server holds a transmit stream after seed");
        let client_cov = self.client.applied_coverage();
        assert!(
            server_cov.gaps.is_empty(),
            "step {step}: server gaps must stay empty on a quiet link: {:?}",
            server_cov.gaps
        );
        assert!(
            client_cov.gaps.is_empty(),
            "step {step}: client gaps must stay empty on a quiet link: {:?}",
            client_cov.gaps
        );
        assert_eq!(
            server_cov.refusals, 0,
            "step {step}: a quiet link records no refusal markers"
        );
        assert_eq!(
            client_cov.refusals, 0,
            "step {step}: a quiet link records no refusal markers"
        );
        assert_eq!(
            server_cov.frontier,
            Some(received),
            "step {step}: server frontier must sit on the last sent tick"
        );
        let applied = self
            .client
            .server_tick()
            .expect("client has a receiving tick");
        assert!(
            received >= applied,
            "step {step}: last_received ({received:?}) must not trail the receiving tick ({applied:?})"
        );
        assert_eq!(
            client_cov.frontier,
            Some(applied),
            "step {step}: client frontier must sit on the receiving tick it has applied"
        );
    }
}

fn assert_quiet_stretch_keeps_both_frontiers(mode: ServerMode) {
    let mut link = Link::new(mode);
    link.seed_replication();
    link.assert_frontiers_agree(0);
    for step in 1..=40 {
        link.step(true);
        if step % 10 == 0 {
            link.assert_frontiers_agree(step);
        }
    }
    link.assert_frontiers_agree(40);
    assert_eq!(link.client_world.proxy().entities().len(), 1);
}

#[test]
fn quiet_resident_stretch_advances_both_frontiers_without_gaps() {
    assert_quiet_stretch_keeps_both_frontiers(ServerMode::Resident);
}

#[test]
fn quiet_pipelined_stretch_advances_both_frontiers_without_gaps() {
    assert_quiet_stretch_keeps_both_frontiers(ServerMode::Pipelined);
}
