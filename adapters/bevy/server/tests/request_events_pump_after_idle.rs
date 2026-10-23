//! RED 108734 adapter leg (Usher 109849): the two-shot
//! `quiet_request_response_after_idle` exchange, one layer up — through
//! `naia-bevy-server`'s `RequestEvents` pump.
//!
//! Shape: a live raw `Server<bevy Entity>` + real `naia_client::Client` over
//! local transport. Every server tick runs the plugin's own pump shape
//! (`receive_all_packets` -> `process_all_packets` -> `take_world_events` ->
//! `RequestEvents::from` into bevy `Messages` -> `send_all_packets`), and
//! requests are observed ONLY via `RequestEvents::read` (never the raw
//! `read_request`). First exchange = control (must pass), 650-tick idle span,
//! second exchange must complete within the e2e's exact 200-tick bound.
//! PREDICTS (Usher 109849): the second exchange reds iff the adapter pump
//! loses requests across idle.

use std::time::Duration;

use bevy_app::App;
use bevy_ecs::{
    entity::Entity,
    message::{MessageCursor, Messages},
    world::World,
};

use naia_bevy_server::{
    events::{AuthEvents, RequestEvents},
    BigMapKey, DriveShape, Plugin, ServerConfig, ServerPluginConfig, Topology,
};
use naia_bevy_shared::{Protocol as BevyProtocol, WorldProxy, WorldProxyMut};
use naia_client::{
    transport::local::{LocalAddrCell, LocalClientSocket, Socket as ClientSocket},
    Client, ClientConfig,
};
use naia_server::{
    transport::local::{LocalServerSocket, Socket as ServerSocket},
    Server as RawServer, ServerMode, UserKey,
};
use naia_shared::{
    transport::local::{LocalTransportHub, FAKE_SERVER_ADDR},
    Instant, ResponseSendKey, TestClock,
};
use naia_test_harness::{
    protocol,
    test_protocol::{OrderedChannel, Position, TestRequest, TestResponse},
    Auth, TestEntity, TestWorld,
};

fn bevy_protocol() -> BevyProtocol {
    let mut p = BevyProtocol::builder();
    p.register_component::<Position>();
    p.tick_interval(Duration::from_micros(100));
    p.build()
}

fn build_app() -> App {
    let mut app = App::new();
    app.add_plugins(Plugin::new(ServerPluginConfig::new(
        ServerConfig::default(),
        bevy_protocol(),
        Topology::Standalone(DriveShape::Resident),
    )));
    app
}

/// Eprint logger so naia `warn!` drop paths (unknown-id, decode) surface in
/// the test log. Temporary rig diagnostics.
struct EprintLogger;
impl log::Log for EprintLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        if record.level() <= log::Level::Warn {
            eprintln!("[rig-log {:?}] {}", record.level(), record.args());
        }
    }
    fn flush(&self) {}
}
static RIG_LOGGER: EprintLogger = EprintLogger;

struct Rig {
    hub: LocalTransportHub,
    server: RawServer<Entity>,
    server_world: World,
    client: Client<TestEntity>,
    client_world: TestWorld,
    app: App,
    user_key: UserKey,
}

fn test_client_config() -> ClientConfig {
    ClientConfig {
        send_handshake_interval: Duration::from_millis(0),
        jitter_buffer: naia_client::JitterBufferType::Bypass,
        ..Default::default()
    }
}

/// One full tick on both sides; the server half mirrors the plugin's own
/// pump exactly (`receive_packets` -> `process_packets` ->
/// `translate_world_events` -> send, systems.rs:141-190,375), translating the
/// combined `Events` into bevy messages through the real `RequestEvents::from`
/// conversion.
fn tick(rig: &mut Rig) {
    TestClock::advance(16);
    rig.hub.process_time_queues();
    let now = Instant::now();

    rig.client.receive_all_packets();
    rig.client
        .process_all_packets(rig.client_world.proxy_mut(), &now);
    rig.client.send_all_packets(rig.client_world.proxy_mut());
    // Harness drain (scenario.rs:1244): take_world_events then
    // take_tick_events — the latter de-jitters buffered packets into the
    // message layer. Without it, responses sit in the jitter buffer forever
    // while acks still flow at intake.
    let _ = rig.client.take_world_events();
    let _ = rig.client.take_tick_events(&now);

    rig.server.receive_all_packets();
    rig.server
        .process_all_packets(rig.server_world.proxy_mut(), &now);
    let mut events = rig.server.take_world_events();
    if events.has_requests() {
        rig.app
            .world_mut()
            .resource_mut::<Messages<RequestEvents>>()
            .write(RequestEvents::from(&mut events));
    }
    if events.has_auths() {
        rig.app
            .world_mut()
            .resource_mut::<Messages<AuthEvents>>()
            .write(AuthEvents::from(&mut events));
    }
    rig.server.send_all_packets(&rig.server_world.proxy());
}

/// Read every request currently visible through the adapter pump.
fn pumped_requests(app: &mut App) -> Vec<(UserKey, ResponseSendKey<TestResponse>, TestRequest)> {
    let mut out = Vec::new();
    let mut cursor = MessageCursor::<RequestEvents>::default();
    let messages = app.world().resource::<Messages<RequestEvents>>();
    for events in cursor.read(messages) {
        out.extend(events.read::<OrderedChannel, TestRequest>());
    }
    out
}

fn rig() -> Rig {
    // Simulated clock must be initialized (mirrors Scenario::new); without
    // this mock time never advances and the handshake never completes.
    TestClock::init(0);
    let _ = log::set_logger(&RIG_LOGGER);
    log::set_max_level(log::LevelFilter::Warn);

    let hub = LocalTransportHub::new(FAKE_SERVER_ADDR.parse().unwrap());
    hub.enable_packet_recording();
    let test_protocol = protocol();

    let mut server = RawServer::<Entity>::new(
        ServerMode::Resident,
        &ServerConfig::default(),
        test_protocol.clone(),
    );
    server.listen(ServerSocket::new(LocalServerSocket::new(hub.clone()), None));

    let room_key = server.create_room().key();

    // Client socket wiring (mirrors Scenario::create_client_socket).
    let (client_addr, auth_req_tx, auth_resp_rx, client_data_tx, client_data_rx) =
        hub.register_client();
    let addr_cell = LocalAddrCell::new();
    addr_cell.set_sync(hub.server_addr());
    let inner_socket = LocalClientSocket::new_with_tokens(
        client_addr,
        hub.server_addr(),
        auth_req_tx,
        auth_resp_rx,
        client_data_tx,
        client_data_rx,
        addr_cell,
        std::sync::Arc::new(parking_lot::Mutex::new(None)),
        std::sync::Arc::new(parking_lot::Mutex::new(None)),
    );
    let socket = ClientSocket::new(inner_socket, None);

    let mut client = Client::new(test_client_config(), test_protocol);
    client.auth(Auth::new("client_a", "pass"));
    client.connect(socket);
    let client_world = TestWorld::default();

    let app = build_app();

    let mut rig = Rig {
        hub,
        server,
        server_world: World::default(),
        client,
        client_world,
        app,
        user_key: UserKey::from_u64(0),
    };

    // Connect: wait for the handshake auth event (the user does not exist
    // before accept), accept, join the room, and wait for the client to
    // report connected. Auths arrive through the same bevy-messages pump as
    // requests, exactly like the plugin's translate_world_events.
    let mut accepted = false;
    for i in 0..300 {
        tick(&mut rig);
        if !accepted {
            let messages = rig.app.world().resource::<Messages<AuthEvents>>();
            let mut cursor = MessageCursor::<AuthEvents>::default();
            if let Some((key, _auth)) = cursor
                .read(messages)
                .flat_map(|batch| batch.read::<Auth>())
                .next()
            {
                rig.user_key = key;
                rig.server.accept_connection(&rig.user_key);
                rig.server.room_mut(&room_key).add_user(&rig.user_key);
                accepted = true;
            }
            if i % 50 == 0 {
                eprintln!(
                    "adapter-rig connect tick {i}: users={} client_connected={} accepted={accepted}",
                    rig.server.users_count(),
                    rig.client.connection_status().is_connected(),
                );
            }
        }
        if accepted && rig.client.connection_status().is_connected() {
            return rig;
        }
    }
    panic!("adapter-rig: client never connected");
}

/// Wait (up to `bound` ticks) for a pumped request whose payload matches
/// `want`, draining the pump each tick like the session plugin does.
fn wait_pumped(
    rig: &mut Rig,
    want: &str,
    bound: u32,
    label: &str,
) -> (UserKey, ResponseSendKey<TestResponse>) {
    for _ in 0..bound {
        tick(rig);
        for (user_key, send_key, request) in pumped_requests(&mut rig.app) {
            if request.query == want {
                return (user_key, send_key);
            }
        }
    }
    panic!("adapter-rig: {label} not visible through RequestEvents within {bound} ticks");
}

fn wait_response(
    rig: &mut Rig,
    key: &naia_shared::ResponseReceiveKey<TestResponse>,
    bound: u32,
    label: &str,
) {
    for i in 0..bound {
        tick(rig);
        if rig.client.has_response(key) {
            return;
        }
        if i % 50 == 0 {
            eprintln!(
                "adapter-rig {label} tick {i}: client_connected={}",
                rig.client.connection_status().is_connected(),
            );
        }
        if i == 5 {
            let mut c2s = Vec::new();
            let mut s2c = Vec::new();
            for (server_to_client, bytes) in rig.hub.take_recorded_packets() {
                if server_to_client {
                    s2c.push(bytes.len());
                } else {
                    c2s.push(bytes.len());
                }
            }
            eprintln!("adapter-rig {label}: wire so far c2s={c2s:?} s2c={s2c:?}");
        }
        if i == 199 {
            let mut s2c_all = Vec::new();
            for (server_to_client, bytes) in rig.hub.take_recorded_packets() {
                if server_to_client {
                    s2c_all.push(bytes.len());
                }
            }
            eprintln!("adapter-rig {label}: full-wait s2c sizes={s2c_all:?}");
        }
        if i == 10 {
            // Bisect probe: did the response land in the client's REQUEST
            // buffer (envelope misrouted as a request)?
            let mut client_events = rig.client.take_world_events();
            let mut stray = Vec::new();
            for (_resp_key, request) in
                client_events.read::<naia_client::RequestEvent<OrderedChannel, TestRequest>>()
            {
                stray.push(request.query.clone());
            }
            eprintln!("adapter-rig {label}: client-side stray requests={stray:?}");
            eprintln!(
                "adapter-rig {label}: client has_messages={} has_requests={}",
                client_events.has_messages(),
                client_events.has_requests(),
            );
        }
    }
    panic!("adapter-rig: {label} not observed by client within {bound} ticks");
}

/// Usher 109849 CARD: two-shot create-then-publish shape through the
/// `RequestEvents` pump. First exchange is the control; after a 650-tick
/// idle span the second exchange must still complete within 200 ticks.
#[test]
fn second_request_after_idle_survives_request_events_pump_within_200_ticks() {
    let mut rig = rig();

    // First exchange (create-shape): control, must pass.
    let first_key = rig
        .client
        .send_request::<OrderedChannel, TestRequest>(&TestRequest::new("first"))
        .expect("first request must be accepted by reliable channel");
    let (_, first_send_key) = wait_pumped(&mut rig, "first", 200, "first request");
    // Oversized payload: a ~230-byte response packet is unmistakable against
    // tick/heartbeat traffic in the wire-size probe below.
    let big = "R".repeat(200);
    assert!(
        rig.server
            .send_response(&first_send_key, &TestResponse::new(&big)),
        "server must answer first request"
    );
    eprintln!(
        "adapter-rig after first send_response: outstanding={}",
        rig.server.outstanding_request_count(&rig.user_key),
    );
    // Reset the wire baseline so the wait below counts only post-response traffic.
    rig.hub.take_recorded_packets();
    wait_response(&mut rig, &first_key, 200, "first response");
    let first = rig
        .client
        .receive_response::<TestResponse>(&first_key)
        .expect("first response must be available");
    assert_eq!(first.result, big);

    // Idle span: the e2e's authoring/save/leave/rejoin/play legs.
    for _ in 0..650 {
        tick(&mut rig);
    }

    // Second exchange (publish-shape): the PREDICTS-red assertion.
    let second_key = rig
        .client
        .send_request::<OrderedChannel, TestRequest>(&TestRequest::new("second"))
        .expect("second request must be accepted by reliable channel");
    let (_, second_send_key) = wait_pumped(&mut rig, "second", 200, "second request");
    assert!(
        rig.server
            .send_response(&second_send_key, &TestResponse::new("ok")),
        "server must answer second request"
    );
    wait_response(&mut rig, &second_key, 200, "second response");
    let second = rig
        .client
        .receive_response::<TestResponse>(&second_key)
        .expect("second response must be available");
    assert_eq!(second.result, "ok");
}
