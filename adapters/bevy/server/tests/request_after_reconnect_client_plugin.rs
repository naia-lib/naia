//! RED 108734 thread-split leg (Usher 110961 GO): same-client reconnect through
//! the REAL bevy client plugin, then a first request that must serialize and
//! be answered within the e2e's exact 200-tick bound.
//!
//! Models the e2e signature: `TestClock` advances only on the driving (main)
//! thread, while bevy pool threads that run `translate_tick_events` see a
//! frozen clock — so no `ClientTickEvent`s are produced, the connected-state
//! send gate stays shut, and the queued request never emits (naia log-quiet).
//! Old-leg traffic in the e2e rode the not-connected always-send windows;
// steady-connected states emit nothing, which is what this rig reproduces.
//!
//! Shape: a live raw `Server<bevy Entity>` + a real bevy `App` running
//! `naia-bevy-client`'s `Plugin`, over local transport. Control roundtrip on
//! the old connection must pass; after a same-client reconnect the first
//! request must be answered within 200 ticks.
//! PREDICTS (mine): RED pre-fix (freeze, empty c2s wire), GREEN after the
//! one-clock-per-app fix.

use std::time::Duration;

use bevy_app::App;
use bevy_ecs::{
    entity::Entity,
    message::{MessageCursor, Messages},
    world::Mut,
};

use naia_bevy_client::{
    events::{ClientTickEvent as BevyClientTickEvent, ServerTickEvent as BevyServerTickEvent},
    ClientWrapper, Plugin as ClientPlugin, ProtocolClientExt,
};
use naia_bevy_server::events::{
    AuthEvents as ServerAuthEvents, RequestEvents as ServerRequestEvents,
};
use naia_bevy_shared::{Protocol as BevyProtocol, WorldProxy, WorldProxyMut};
use naia_client::{
    transport::local::{LocalAddrCell, LocalClientSocket, Socket as ClientSocket},
    ClientConfig, JitterBufferType,
};
use naia_client::{ClientTickEvent as CoreClientTickEvent, ServerTickEvent as CoreServerTickEvent};
use naia_server::{
    transport::local::{LocalServerSocket, Socket as ServerSocket},
    Server as RawServer, ServerMode, UserKey,
};
use naia_shared::{
    transport::local::{LocalTransportHub, FAKE_SERVER_ADDR},
    Instant, TestClock,
};
use naia_shared::{ChannelDirection, ChannelMode, ReliableSettings, TickBufferSettings};
use naia_test_harness::{
    protocol,
    test_protocol::{
        Auth, EntityCommandMessage, ImmutableLabel, ImmutableSeed, LargeTestMessage,
        OrderedChannel, Position, ReliableChannel, RequestResponseChannel, SequencedChannel,
        ServerToClientChannel, TestMatchState, TestMessage, TestPlayerSelection, TestRequest,
        TestResponse, TestScore, TickBufferedChannel, UnorderedChannel, UnreliableChannel,
        Velocity,
    },
};

fn bevy_protocol() -> BevyProtocol {
    // Mirror naia_test_harness::protocol() exactly: the client plugin's
    // handshake must fingerprint-match the raw server's protocol.
    let mut p = BevyProtocol::builder();
    p.add_component::<Entity, Position>();
    p.add_component::<Entity, Velocity>();
    p.add_component::<Entity, ImmutableLabel>();
    p.add_component::<Entity, ImmutableSeed>();
    p.add_resource::<TestScore>();
    p.add_resource::<TestMatchState>();
    p.add_resource::<TestPlayerSelection>();
    p.add_message::<Auth>();
    p.add_message::<TestMessage>();
    p.add_message::<LargeTestMessage>();
    p.add_message::<EntityCommandMessage>();
    p.add_message::<TestRequest>();
    p.add_message::<TestResponse>();
    p.add_channel::<ReliableChannel>(
        ChannelDirection::Bidirectional,
        ChannelMode::UnorderedReliable(ReliableSettings::default()),
    );
    p.add_channel::<UnreliableChannel>(
        ChannelDirection::Bidirectional,
        ChannelMode::UnorderedUnreliable,
    );
    p.add_channel::<OrderedChannel>(
        ChannelDirection::Bidirectional,
        ChannelMode::OrderedReliable(ReliableSettings::default()),
    );
    p.add_channel::<UnorderedChannel>(
        ChannelDirection::Bidirectional,
        ChannelMode::UnorderedReliable(ReliableSettings::default()),
    );
    p.add_channel::<SequencedChannel>(
        ChannelDirection::Bidirectional,
        ChannelMode::SequencedReliable(ReliableSettings::default()),
    );
    p.add_channel::<TickBufferedChannel>(
        ChannelDirection::ClientToServer,
        ChannelMode::TickBuffered(TickBufferSettings::default()),
    );
    p.add_channel::<RequestResponseChannel>(
        ChannelDirection::Bidirectional,
        ChannelMode::UnorderedReliable(ReliableSettings::default()),
    );
    p.add_channel::<ServerToClientChannel>(
        ChannelDirection::ServerToClient,
        ChannelMode::UnorderedReliable(ReliableSettings::default()),
    );
    p.enable_client_authoritative_entities();
    p.tick_interval(Duration::from_micros(100));
    p.build()
}

fn test_client_config() -> ClientConfig {
    ClientConfig {
        send_handshake_interval: Duration::from_millis(0),
        jitter_buffer: JitterBufferType::Bypass,
        ..Default::default()
    }
}

struct Rig {
    hub: LocalTransportHub,
    server: RawServer<Entity>,
    server_world: bevy_ecs::world::World,
    app: App,
    // Mirrors the `CachedClientTickEventsState` cursor `send_packets`
    // (systems.rs) reads: the send gate passes while connected only when
    // this cursor observes a ClientTickEvent.
    tick_cursor: MessageCursor<BevyClientTickEvent<Entity>>,
    room_key: naia_server::RoomKey,
    user_key: Option<UserKey>,
    auth_events: Messages<ServerAuthEvents>,
    request_events: Messages<ServerRequestEvents>,
}

fn client_socket(hub: &LocalTransportHub) -> ClientSocket {
    // Mirrors Scenario::create_client_socket.
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
    ClientSocket::new(inner_socket, None)
}

/// One full tick on both sides.
///
/// The client half replicates the REAL plugin pipeline call-for-call in
/// plugin order — `receive_packets` → `translate_tick_events` →
/// `process_packets` → `send_packets` (`adapters/bevy/client/src/systems.rs`)
/// — with ONE difference, the variable under test: `translate_tick_events`
/// runs on a freshly spawned thread per tick, like the e2e bevy pool worker
/// executing the client app without ever running the scenario's
/// `TestClock::advance`. The per-tick `TestClock::advance(16)` happens ONLY
/// on the driving thread (mirrors `Scenario::tick_*`), so the spawned
/// thread's `TestClock` lazy-inits on first read and stays frozen. If the
/// send gate depends on tick events produced from that clock, the
/// post-rejoin request stalls inside the 200-tick bound — the RED this rig
/// is built to catch.
/// The body of `translate_tick_events` (`adapters/bevy/client/src/systems.rs`),
/// callable on any thread holding the world's unique borrow.
fn translate_on(world: &mut bevy_ecs::world::World) {
    let now = Instant::now();
    world.resource_scope(|world, mut wrapper: Mut<ClientWrapper<Entity>>| {
        let mut events = wrapper.client.take_tick_events(&now);
        if events.is_empty() {
            return;
        }
        if events.has::<CoreClientTickEvent>() {
            let mut writer = world.resource_mut::<Messages<BevyClientTickEvent<Entity>>>();
            for tick in events.read::<CoreClientTickEvent>() {
                writer.write(BevyClientTickEvent::<Entity>::new(tick));
            }
        }
        if events.has::<CoreServerTickEvent>() {
            let mut writer = world.resource_mut::<Messages<BevyServerTickEvent<Entity>>>();
            for tick in events.read::<CoreServerTickEvent>() {
                writer.write(BevyServerTickEvent::<Entity>::new(tick));
            }
        }
    });
}

fn tick(rig: &mut Rig) {
    TestClock::advance(16);
    rig.hub.process_time_queues();

    // 1. receive_packets, on the driving thread.
    rig.app
        .world_mut()
        .resource_mut::<ClientWrapper<Entity>>()
        .client
        .receive_all_packets();

    // 2. translate_tick_events, on a spawned thread (frozen clock).
    // `std::thread::scope` carries the world's unique borrow across and
    // rejoins it each tick; `resource_scope` replays the system's borrow
    // split exactly.
    std::thread::scope(|s| {
        let world = &mut *rig.app.world_mut();
        s.spawn(move || {
            translate_on(world);
        })
        .join()
        .unwrap();
    });

    // 3. process_packets, on the driving thread.
    let now = Instant::now();
    rig.app
        .world_mut()
        .resource_scope(|world, mut wrapper: Mut<ClientWrapper<Entity>>| {
            wrapper.client.process_all_packets(world.proxy_mut(), &now);
        });

    // 4. send_packets' gate, on the driving thread: disconnected always
    // sends; connected sends only when the tick cursor observed a
    // ClientTickEvent since the last send.
    let should_send = {
        let world = rig.app.world_mut();
        if !world
            .resource::<ClientWrapper<Entity>>()
            .client
            .connection_status()
            .is_connected()
        {
            true
        } else {
            let messages = world.resource::<Messages<BevyClientTickEvent<Entity>>>();
            rig.tick_cursor.read(messages).count() > 0
        }
    };
    if should_send {
        rig.app
            .world_mut()
            .resource_scope(|world, mut wrapper: Mut<ClientWrapper<Entity>>| {
                wrapper.client.send_all_packets(world.proxy());
            });
    }

    let now = Instant::now();
    rig.server.receive_all_packets();
    rig.server
        .process_all_packets(rig.server_world.proxy_mut(), &now);
    let mut events = rig.server.take_world_events();
    if events.has_requests() {
        rig.request_events
            .write(ServerRequestEvents::from(&mut events));
    }
    if events.has_auths() {
        rig.auth_events.write(ServerAuthEvents::from(&mut events));
    }
    rig.server.send_all_packets(&rig.server_world.proxy());
}

fn rig() -> Rig {
    // Simulated clock lives on the driving thread only (mirrors
    // Scenario::new). Pool threads that execute bevy systems see a frozen
    // clock — that asymmetry is the mechanism under test.
    TestClock::init(0);

    let hub = LocalTransportHub::new(FAKE_SERVER_ADDR.parse().unwrap());
    hub.enable_packet_recording();

    let mut server = RawServer::<Entity>::new(
        ServerMode::Resident,
        &naia_server::ServerConfig::default(),
        protocol(),
    );
    server.listen(ServerSocket::new(LocalServerSocket::new(hub.clone()), None));
    let room_key = server.create_room().key();

    let mut app = App::new();
    app.add_plugins(ClientPlugin::<Entity>::new(
        test_client_config(),
        bevy_protocol(),
    ));

    Rig {
        hub,
        server,
        server_world: bevy_ecs::world::World::default(),
        app,
        tick_cursor: MessageCursor::default(),
        room_key,
        user_key: None,
        auth_events: Messages::default(),
        request_events: Messages::default(),
    }
}

fn wrapper<'a>(rig: &'a mut Rig) -> bevy_ecs::world::Mut<'a, ClientWrapper<Entity>> {
    rig.app.world_mut().resource_mut::<ClientWrapper<Entity>>()
}

fn connect_client(rig: &mut Rig, label: &str) {
    let socket = client_socket(&rig.hub);
    {
        let mut w = wrapper(rig);
        w.client.auth(Auth::new("client_a", "pass"));
        w.client.connect(socket);
    }
    let mut accepted = false;
    for i in 0..300 {
        tick(rig);
        // Server side: accept the incoming auth and join the room.
        let mut auth_cursor = bevy_ecs::message::MessageCursor::<ServerAuthEvents>::default();
        let mut got_auth = None;
        for batch in auth_cursor.read(&rig.auth_events) {
            for (key, _auth) in batch.read::<Auth>() {
                got_auth = Some(key);
            }
        }
        if let Some(key) = got_auth {
            if !accepted {
                rig.server.accept_connection(&key);
                rig.server.room_mut(&rig.room_key).add_user(&key);
                rig.user_key = Some(key);
                accepted = true;
            }
        }
        let connected = wrapper(rig).client.connection_status().is_connected();
        if accepted && connected {
            return;
        }
        if i % 50 == 0 {
            eprintln!(
                "threadsplit-rig {label} connect tick {i}: accepted={accepted} connected={connected}"
            );
        }
    }
    panic!("threadsplit-rig: {label} client never connected");
}

/// Answer every pending request whose payload matches `want`, immediately
/// (mirrors the session service answering publish inline).
fn answer_pending(rig: &mut Rig, want: &str) {
    let mut cursor = bevy_ecs::message::MessageCursor::<ServerRequestEvents>::default();
    let mut send_keys = Vec::new();
    for batch in cursor.read(&rig.request_events) {
        for (_user_key, send_key, request) in batch.read::<RequestResponseChannel, TestRequest>() {
            if request.query == want {
                send_keys.push(send_key);
            }
        }
    }
    for send_key in send_keys {
        if !rig
            .server
            .send_response(&send_key, &TestResponse::new("ok"))
        {
            eprintln!("threadsplit-rig: server refused answer for a received request");
        }
    }
}

/// Queue a request and wait up to `bound` ticks for its response, answering
/// server-side inline. Returns on success; panics with wire evidence on
/// expiry (the e2e's waiter shape).
fn request_response_within(
    rig: &mut Rig,
    query: &str,
    bound: u32,
    label: &str,
) -> naia_client::shared::ResponseReceiveKey<TestResponse> {
    let response_key = {
        let mut w = wrapper(rig);
        w.client
            .send_request::<RequestResponseChannel, TestRequest>(&TestRequest::new(query))
            .expect("request must be accepted by reliable channel")
    };
    // Reset the wire baseline so the expiry report counts only this exchange.
    rig.hub.take_recorded_packets();
    for i in 0..bound {
        tick(rig);
        answer_pending(rig, query);
        {
            let mut w = wrapper(rig);
            if let Some(response) = w.client.receive_response::<TestResponse>(&response_key) {
                assert_eq!(response.result, "ok");
                return response_key;
            }
        }
        if i % 50 == 0 {
            eprintln!(
                "threadsplit-rig {label} tick {i}: connected={}",
                wrapper(rig).client.connection_status().is_connected()
            );
        }
    }
    // Expiry evidence: c2s sizes prove whether the client ever emitted
    // (empty == queue-side freeze, the e2e signature) or emitted-but-lost.
    let mut c2s = Vec::new();
    let mut s2c = Vec::new();
    for (server_to_client, bytes) in rig.hub.take_recorded_packets() {
        if server_to_client {
            s2c.push(bytes.len());
        } else {
            c2s.push(bytes.len());
        }
    }
    panic!("threadsplit-rig: {label} not answered within {bound} ticks (c2s={c2s:?} s2c={s2c:?})");
}

/// RED 108734 thread-split leg: the e2e's first-bad exchange was the FIRST
/// request on a FRESH post-rejoin connection. Control roundtrip on the old
/// connection must pass; after a same-client reconnect the first request
/// must be answered within the e2e's exact 200-tick bound.
#[test]
fn request_after_same_client_reconnect_completes_within_200_ticks_client_plugin() {
    let mut rig = rig();

    connect_client(&mut rig, "initial");

    // Control: first exchange on the old connection (must pass — proves the
    // rig, the protocols, and the answer path are healthy).
    request_response_within(&mut rig, "control", 200, "control");

    // Old editor leaves: the client disconnects (same wrapper, same App —
    // the e2e rejoin shape). The server keeps the stale user; the rejoin
    // handshakes as a new user, like the e2e's new cell.
    wrapper(&mut rig).client.disconnect();
    for _ in 0..60 {
        tick(&mut rig);
        if !wrapper(&mut rig).client.connection_status().is_connected() {
            break;
        }
    }
    assert!(
        !wrapper(&mut rig).client.connection_status().is_connected(),
        "client must observe the disconnect"
    );

    // Same client reconnects to the new cell.
    connect_client(&mut rig, "rejoin");

    // Publish-shape: first request on the fresh connection within 200 ticks.
    request_response_within(&mut rig, "post-rejoin", 200, "post-rejoin");
}
