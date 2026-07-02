//! G8/P5 — Resident == Pipelined byte identity with live ACK traffic.
//!
//! `g9pre_resident_pipelined_byte_identity` proves the send payloads match when
//! the controlled mutation tick is isolated after setup settles. This test closes
//! the remaining boundary obligation from `MISSION_PIPELINE_API_BOUNDARY.md`: run
//! the normal harness connection in both `ServerMode`s while the client receives
//! updates and sends ACKs back to the server between mutation ticks. The full
//! local-transport trace (direction + bytes) must be identical.

use std::time::Duration;

use naia_client::{ClientConfig, JitterBufferType};
use naia_server::{ReplicationConfig, ServerConfig, ServerMode};

use naia_test_harness::{
    protocol, Auth, EntityKey, ExpectResult, Position, Scenario, TraceDirection, TracePacket,
};

mod _helpers;
use _helpers::client_connect;

#[derive(Clone, Copy)]
struct Mutation {
    x: f32,
    y: f32,
}

const MUTATIONS: &[Mutation] = &[
    Mutation { x: 10.0, y: 20.0 },
    Mutation { x: 11.0, y: 20.0 },
    Mutation { x: 11.0, y: 22.0 },
    Mutation { x: 30.0, y: 40.0 },
];

fn client_config() -> ClientConfig {
    let mut config = ClientConfig::default();
    config.send_handshake_interval = Duration::from_millis(0);
    config.jitter_buffer = JitterBufferType::Bypass;
    config
}

fn setup(mode: ServerMode) -> (Scenario, EntityKey) {
    let mut scenario = Scenario::new(mode);
    let proto = protocol();
    scenario.server_start(ServerConfig::default(), proto.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|s| s.create_room().key()));

    let client_key = client_connect(
        &mut scenario,
        &room_key,
        "client",
        Auth::new("user", "password"),
        client_config(),
        proto,
    );

    let (entity_key, ()): (EntityKey, ()) = scenario.mutate(|ctx| {
        ctx.server(|s| {
            s.spawn(|mut entity| {
                entity
                    .configure_replication(ReplicationConfig::public())
                    .insert_component(Position::new(0.0, 0.0))
                    .enter_room(&room_key);
            })
        })
    });

    scenario.expect(|ctx| {
        ctx.client(client_key, |client| {
            client.has_entity(&entity_key).then_some(())
        })
    });

    // Leave setup out of the trace, but ensure both modes begin from the same
    // steady connection history before the controlled ACK-bearing mutation loop.
    for _ in 0..8 {
        tick_once(&mut scenario);
    }

    (scenario, entity_key)
}

fn apply_mutation(scenario: &mut Scenario, entity_key: &EntityKey, mutation: Mutation) {
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            let mut entity = server
                .entity_mut(entity_key)
                .expect("server entity must exist for mutation");
            let mut position = entity
                .component::<Position>()
                .expect("server entity must still have Position");
            *position.x = mutation.x;
            *position.y = mutation.y;
        });
    });
}

fn tick_once(scenario: &mut Scenario) {
    match scenario.expect_once(|_| ExpectResult::Passed(())) {
        ExpectResult::Passed(()) => {}
        ExpectResult::NotYet => unreachable!("closure always passes"),
        ExpectResult::Failed(message) => panic!("fixed tick failed: {message}"),
    }
}

fn run_trace(mode: ServerMode) -> Vec<TracePacket> {
    let (mut scenario, entity_key) = setup(mode);
    scenario.enable_trace_capture();

    for mutation in MUTATIONS {
        apply_mutation(&mut scenario, &entity_key, *mutation);

        // Tick 1: client receives the server update and sends its ACK.
        // Tick 2: server processes the ACK and sends the next normal packet.
        // The fixed cadence keeps resident and pipelined traces aligned and
        // prevents an expect loop from hiding mode-specific timing drift.
        tick_once(&mut scenario);
        tick_once(&mut scenario);
    }

    scenario.take_trace().packets
}

fn trace_signature(trace: Vec<TracePacket>) -> Vec<(TraceDirection, Vec<u8>)> {
    trace
        .into_iter()
        .map(|packet| (packet.direction, packet.bytes))
        .collect()
}

fn hexdump(label: &str, trace: &[(TraceDirection, Vec<u8>)]) {
    println!("{label}: {} packet(s)", trace.len());
    for (index, (direction, bytes)) in trace.iter().enumerate() {
        let dir = match direction {
            TraceDirection::ClientToServer => "c2s",
            TraceDirection::ServerToClient => "s2c",
        };
        let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        println!("  [{index:02}] {dir} len={:>3} {hex}", bytes.len());
    }
}

#[test]
fn g8_real_ack_resident_pipelined_byte_identity() {
    let resident = trace_signature(run_trace(ServerMode::Resident));
    let pipelined = trace_signature(run_trace(ServerMode::Pipelined));

    hexdump("resident", &resident);
    hexdump("pipelined", &pipelined);

    assert_eq!(
        resident, pipelined,
        "resident and pipelined server modes must produce byte-identical packet \
         traces while real client ACKs are flowing between controlled mutation \
         ticks"
    );
}
