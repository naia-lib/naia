//! Tick227 send-gate regression (Usher 108045).
//!
//! ## The bug this gates against
//!
//! On the pipelined-oracle shape (`PipelinedServer::{receive, send}` with no
//! workers: the synchronous determinism path), the D8 send-prep ACK drain is
//! the FIRST consumer of the ACK samples this tick's recv pushed — the
//! pipelined recv decodes but never drains (the coordinator must not drain;
//! the send worker owns that in worker shape). Pre-fix, nothing finalized
//! those freshly-drained deliveries before the D9 plan ran:
//! `drain_all_acks` only records (notification buffered, ledger trimmed) while
//! the delivered state the plan reads is applied by `process_received_commands`
//! (recv step 3, which runs BEFORE the drain). The D9 plan therefore saw the
//! component as undelivered and suppressed the update even though the ACK had
//! arrived that same tick; the update slipped exactly one tick (finalize lands
//! at the next recv). Post-fix, `finalize_drained_deliveries` runs between the
//! D8 drain and the D9 plan, and the update emits in the race tick.
//!
//! ## Why this test drives the bracket, not Scenario
//!
//! `Scenario` (and `Server::{receive_all_packets, send_all_packets}` even in
//! `Pipelined` mode) reassembles the monolithic resident engine, whose recv
//! already drains AND finalizes before the plan — the race is unconstructible
//! there (measured: the Scenario form of this test passes with AND without the
//! fix). Only the true `receive`/`send` bracket expresses D8-drains-first, so
//! this test drives it manually, mirroring `g9pre`'s `DirectScopeRun`.
//!
//! ## Setup
//!
//! A server-owned `public` `Position` entity spawned with V0; V1 is staged
//! ticklessly right after the spawn bracket, so the race tick is the first
//! plan that wants the V1 update while the spawn itself is still
//! unfinalized-delivered: the client receives the spawn and its spawn-ACK
//! reaches the server socket loop during the race tick's recv, and the D8
//! drain is the first consumer. The D9 update plan gates on the delivered
//! world (`is_component_updatable`: the component must exist delivered), so
//! pre-fix the plan suppresses V1 (spawn drained but not finalized) and emits
//! a tick later; post-fix the finalize runs before the plan and V1 emits in
//! the race tick. The hub delivers each side's packets at the next bracket's
//! start, so the spawn-ACK the client sends at B reaches the server at C —
//! after C's recv-phase Step-3 finalize but before C's D8 drain. The gate
//! itself is asserted on the wire: post-fix bracket C carries V1's update
//! (Data) and D carries none; pre-fix C carries only a heartbeat and the
//! update slips to D via the next recv's Step-3 backstop finalize. Client
//! position reads are fixture health only (spawn lands at C with V0 either
//! way; V1 converges by pump G either way — the client's tick-buffered apply
//! lands on its own beat regardless of which tick transmitted).

use std::{net::SocketAddr, sync::Arc, time::Duration};

use naia_client::{
    transport::local::{LocalAddrCell, LocalClientSocket, Socket as ClientSocket},
    Client, ClientConfig, JitterBufferType,
};
use naia_server::{
    transport::local::{LocalServerSocket, Socket as ServerSocket},
    ConnectEvent, ReplicationConfig, Server, ServerConfig, ServerMode, UserKey,
};
use naia_shared::{
    transport::local::LocalTransportHub, Instant, TestClock, WorldMutType, WorldRefType,
};

use naia_test_harness::{protocol, Auth, Position, TestEntity, TestWorld};

use parking_lot::Mutex;

const FAKE_SERVER_ADDR: &str = "127.0.0.1:54627";
const V0: (f32, f32) = (0.0, 0.0);
const V1: (f32, f32) = (10.0, 11.0);

struct OracleRun {
    hub: LocalTransportHub,
    server: Server<TestEntity>,
    server_world: TestWorld,
    client: Client<TestEntity>,
    client_world: TestWorld,
}

impl OracleRun {
    fn new() -> Self {
        TestClock::init(0);
        let server_addr: SocketAddr = FAKE_SERVER_ADDR.parse().unwrap();
        let hub = LocalTransportHub::new(server_addr);

        // Pipelined-oracle shape: the true receive/send bracket where the D8
        // send-prep ACK drain is the first consumer of this tick's recv.
        let mut server = Server::new(ServerMode::Pipelined, &ServerConfig::default(), protocol());
        let server_socket = ServerSocket::new(LocalServerSocket::new(hub.clone()), None);
        server.listen(server_socket);

        let client_config = ClientConfig {
            send_handshake_interval: Duration::from_millis(0),
            jitter_buffer: JitterBufferType::Bypass,
            ..ClientConfig::default()
        };

        let (client_addr, auth_req_tx, auth_resp_rx, client_data_tx, client_data_rx) =
            hub.register_client();
        let addr_cell = LocalAddrCell::new();
        addr_cell.set_sync(hub.server_addr());
        let identity_token = Arc::new(Mutex::new(None));
        let rejection_code = Arc::new(Mutex::new(None));
        let inner_socket = LocalClientSocket::new_with_tokens(
            client_addr,
            hub.server_addr(),
            auth_req_tx,
            auth_resp_rx,
            client_data_tx,
            client_data_rx,
            addr_cell,
            identity_token,
            rejection_code,
        );
        let socket = ClientSocket::new(inner_socket, None);
        let mut client = Client::new(client_config, protocol());
        client.auth(Auth::new("user", "password"));
        client.connect(socket);

        hub.enable_packet_recording();
        Self {
            hub,
            server,
            server_world: TestWorld::default(),
            client,
            client_world: TestWorld::default(),
        }
    }

    /// Classic full-tick driver (monolithic reassembly under the hood).
    /// Used for connect + setup only; the race itself runs on `tick_bracket`.
    /// Mirrors `DirectScopeRun::tick_classic` exactly: the client processes
    /// unconditionally (no Connecting gate) so setup packets that arrive
    /// before the handshake completes are still applied.
    fn tick_classic(&mut self) {
        TestClock::advance(16);
        let now = Instant::now();
        self.hub.process_time_queues();

        self.client.receive_all_packets();
        self.client
            .process_all_packets(self.client_world.proxy_mut(), &now);
        self.client.send_all_packets(self.client_world.proxy_mut());
        // Drain tick events every tick (mirrors Scenario): without this the
        // client's receiving tick never advances and Data never applies.
        let _ = self.client.take_tick_events(&now);

        self.server.receive_all_packets();
        self.server
            .process_all_packets(self.server_world.proxy_mut(), &now);
        self.server.send_all_packets(&self.server_world.proxy());
        // Drain server tick events every tick (mirrors Scenario).
        let _ = self.server.take_tick_events(&now);
    }

    /// True oracle bracket: client phases, then the split coord/recv/send path
    /// (`PipelinedServer::{receive, send}` — D8 drains first, D9 plans).
    /// No workers are running, so `send` transmits inline on this thread.
    fn tick_bracket(&mut self) {
        TestClock::advance(16);
        let now = Instant::now();
        self.hub.process_time_queues();

        // Unconditional client processing + per-tick event drains (mirrors
        // tick_classic: without take_tick_events the client's tick clock
        // stalls and Data packets never materialize).
        self.client.receive_all_packets();
        self.client
            .process_all_packets(self.client_world.proxy_mut(), &now);
        self.client
            .send_all_packets(self.client_world.proxy_mut());
        let _ = self.client.take_tick_events(&now);

        self.server.receive(self.server_world.proxy_mut());
        self.server.send(&self.server_world.proxy());
        let _ = self.server.take_tick_events(&now);
    }

    fn connect(&mut self) -> UserKey {
        for _ in 0..64 {
            self.tick_classic();
            let mut events = self.server.take_world_events();
            let auths: Vec<_> = events
                .read::<naia_server::AuthEvent<Auth>>()
                .map(|(user_key, _auth)| user_key)
                .collect();
            for user_key in auths {
                self.server.accept_connection(&user_key);
            }
            if let Some(user_key) = events.read::<ConnectEvent>().next() {
                return user_key;
            }
        }
        panic!("client did not connect");
    }

    fn read_position(&self) -> Option<(f32, f32)> {
        let proxy = self.client_world.proxy();
        let local = proxy.entities().into_iter().next()?;
        self.client
            .entity(proxy, &local)
            .component::<Position>()
            .map(|p| (*p.x, *p.y))
    }

    fn stage_value(&mut self, entity: &TestEntity, v: (f32, f32)) {
        if let Some(mut pos) = self
            .server
            .entity_mut(self.server_world.proxy_mut(), entity)
            .component::<Position>()
        {
            *pos.x = v.0;
            *pos.y = v.1;
        } else {
            panic!("fixture broken: Position missing at staging");
        }
    }
}

#[test]
fn drained_ack_is_finalized_before_same_tick_plan() {
    let mut run = OracleRun::new();

    // Room first (mirrors the Scenario order: room, connect, add-user, spawn).
    let room_key = run.server.create_room().key();
    let user_key = run.connect();
    run.server.room_mut(&room_key).add_user(&user_key);

    // Server-owned public entity (mirrors DirectScopeRun::setup_scoped_entity:
    // world-spawn + enable + configure + insert + enter room).
    let server_entity = run.server_world.proxy_mut().spawn_entity();
    run.server
        .entity_mut(run.server_world.proxy_mut(), &server_entity)
        .enable_replication()
        .configure_replication(ReplicationConfig::public())
        .insert_component(Position::new(V0.0, V0.1))
        .enter_room(&room_key);

    // Bracket A: the split `send` drains the setup lifecycle op cleanly and
    // transmits the spawn (V0). No read yet — the client applies at C.
    run.hub.take_recorded_packets();
    run.tick_bracket();
    let _ = run.hub.take_recorded_packets();

    // Stage V1 ticklessly while the spawn is in flight and unacked.
    run.stage_value(&server_entity, V1);

    // Bracket B (race tick): the client receives the spawn and its spawn-ACK
    // reaches the server socket loop during recv; the D8 drain is the first
    // consumer. Post-fix the finalize runs before the D9 plan, which emits
    // V1; pre-fix the plan sees the spawn undelivered and suppresses V1.
    run.tick_bracket();
    let _ = run.hub.take_recorded_packets();

    // Bracket C: the spawn is applied (V0 either way — fixture health: the
    // spawn landed). Post-fix V1 transmits here; pre-fix only a heartbeat
    // goes out (the plan suppressed V1).
    run.tick_bracket();
    let at_c = run.read_position();
    let wire_c: Vec<Vec<u8>> = run
        .hub
        .take_recorded_packets()
        .into_iter()
        .filter_map(|(s2c, b)| s2c.then_some(b))
        .collect();
    assert_eq!(
        at_c,
        Some(V0),
        "fixture broken: spawn never applied (at_c={at_c:?})"
    );

    // Bracket D: V1 approaches the client (post-fix) or transmits here via
    // the next recv's Step-3 backstop finalize (pre-fix). The client still
    // holds V0 either way.
    run.tick_bracket();
    debug_assert_eq!(run.read_position(), Some(V0));
    let wire_d: Vec<Vec<u8>> = run
        .hub
        .take_recorded_packets()
        .into_iter()
        .filter_map(|(s2c, b)| s2c.then_some(b))
        .collect();

    // Bracket E: the client still holds V0 either way (V1 applies at F).
    run.tick_bracket();
    let _ = run.read_position();
    let _ = run.hub.take_recorded_packets();
    // Pump brackets F/G: the client applies V1 (post-fix F, pre-fix G).
    let mut pump = [Some(V0); 2];
    for slot in pump.iter_mut() {
        run.tick_bracket();
        *slot = run.read_position();
    }
    assert_eq!(
        pump[1],
        Some(V1),
        "fixture broken: client never converged to V1 (pump={pump:?})"
    );

    // THE GATE, on the wire: bracket C must carry V1's update and bracket D
    // must not. An update-bearing Data packet is unmistakable by size here —
    // the empty Data packet the server emits when the plan suppresses the
    // update is 14 bytes, while V1's update (one public Position) is 24.
    // (Threshold, not exact: the ack bitfield in the header can wobble by a
    // byte or two run to run; the 10-byte gap does not close.)
    fn carries_update(packets: &[Vec<u8>]) -> bool {
        packets.iter().any(|b| b.len() > 20)
    }
    assert!(
        carries_update(&wire_c),
        "send gate stalled: the race-tick D9 plan did not emit V1 even though \
         the spawn ACK was drained that same tick — bracket C carries no \
         update (s2c lens {:?}), so the update slipped a tick",
        wire_c.iter().map(|b| b.len()).collect::<Vec<_>>()
    );
    assert!(
        !carries_update(&wire_d),
        "send gate misattributed: bracket D also carries an update (s2c lens \
         {:?}) — the emission did not happen at the race tick C",
        wire_d.iter().map(|b| b.len()).collect::<Vec<_>>()
    );
}
