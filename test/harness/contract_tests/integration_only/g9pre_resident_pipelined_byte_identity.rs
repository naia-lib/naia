//! G9pre SPIKE — Resident ≡ Pipelined wire-output byte-identity.
//!
//! MISSION_PIPELINE_API_BOUNDARY §2h H1: before `ServerMode::{Resident,Pipelined}`
//! collapses to a single knob (G9) and the determinism/desync oracle is allowed
//! to run in `Resident` mode to validate the `Pipelined` production path, we must
//! KNOW whether the two send paths put the same bytes on the wire.
//!
//! ## The two paths under test (both funnel through `SendState`)
//!
//! - **Resident** — `Server::send_all_packets(&live_world)` =
//!   `prepare_send_job(&live)` + `transmit_send_job(live)` back-to-back, same tick,
//!   reading the LIVE world (`world_server.rs:1018-1024`).
//! - **Pipelined** — `prepare_send_job(&live)` at the freeze point (captures the
//!   per-property `DiffMask`, clears the live mask) + `transmit_send_job(snapshot)`
//!   reading a `SnapshotWorld` captured at that freeze point. This is the
//!   MISSION_TICK_FLOOR Lever-3 one-tick-lag shape cyberlith's park window drives.
//!
//! For a SINGLE mutation tick driven one-tick-at-a-time, the two paths are
//! tick-aligned (no residual lag): both emit exactly one server→client send
//! carrying the new value(s). The only structural difference is resident reads the
//! live world while pipelined reads the frozen `SnapshotWorld`. If the snapshot
//! faithfully carries the same component values, and the frozen `DiffMask` matches
//! the live mask, the serialized REPLICATION PAYLOAD must be identical — and, since
//! both runs share an identical packet history through handshake+spawn (each
//! `Scenario::new` resets the thread-local `TestClock` to 0), so must the packet
//! ENVELOPE (sequence id, ack bitfield).
//!
//! ## What this spike measures
//!
//! Two independent `Scenario`s are stepped in lockstep through identical setup.
//! Trace capture is enabled AFTER the spawn settles, isolating the controlled
//! mutation tick. We then diff the server→client wire bytes for each case across
//! the diff-mask space:
//!   - both properties of one component dirty (full mask),
//!   - a single property dirty (partial mask),
//!   - multiple entities dirty in one tick.
//!
//! The spike prints both traces (hex) under `--nocapture` and asserts full-packet
//! byte-identity (envelope + payload) for every case.

#![allow(unused_imports)]

use std::{net::SocketAddr, sync::Arc, time::Duration};

use naia_client::{
    transport::local::{LocalAddrCell, LocalClientSocket, Socket as ClientSocket},
    Client, ClientConfig, JitterBufferType,
};
use naia_server::{
    transport::local::{LocalServerSocket, Socket as ServerSocket},
    ConnectEvent, ReplicationConfig, Server, ServerConfig, ServerMode,
};
use naia_shared::{
    transport::local::LocalTransportHub, ComponentKind, Instant, Replicate, SnapshotWorld,
    TestClock, WorldMutType, WorldRefType,
};

use naia_test_harness::{
    protocol, Auth, ClientKey, EntityKey, ExpectResult, Position, Scenario, TestEntity, TestScore,
    TestWorld, TraceDirection, TracePacket,
};
use parking_lot::Mutex;

mod _helpers;
use _helpers::client_connect;

const NUM_ENTITIES: usize = 2;

/// One entity's mutation in a case: which properties change, to what values.
#[derive(Clone, Copy)]
struct EntityMutation {
    index: usize,
    set_x: Option<f32>,
    set_y: Option<f32>,
}

/// A test case = a set of per-entity mutations applied in a single tick.
struct Case {
    label: &'static str,
    mutations: Vec<EntityMutation>,
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            label: "full-mask: both props of one entity",
            mutations: vec![EntityMutation {
                index: 0,
                set_x: Some(123.0),
                set_y: Some(456.0),
            }],
        },
        Case {
            label: "partial-mask: only x of one entity",
            mutations: vec![EntityMutation {
                index: 0,
                set_x: Some(777.0),
                set_y: None,
            }],
        },
        Case {
            label: "partial-mask: only y of one entity",
            mutations: vec![EntityMutation {
                index: 1,
                set_x: None,
                set_y: Some(888.0),
            }],
        },
        Case {
            label: "multi-entity: both entities, mixed masks",
            mutations: vec![
                EntityMutation {
                    index: 0,
                    set_x: Some(11.0),
                    set_y: Some(22.0),
                },
                EntityMutation {
                    index: 1,
                    set_x: Some(33.0),
                    set_y: None,
                },
            ],
        },
    ]
}

fn client_config() -> ClientConfig {
    let mut c = ClientConfig::default();
    c.send_handshake_interval = Duration::from_millis(0);
    c.jitter_buffer = JitterBufferType::Bypass;
    c
}

fn direct_server_config() -> ServerConfig {
    ServerConfig::default()
}

/// Identical setup for both modes: connect one client, spawn NUM_ENTITIES
/// server-owned public `Position` entities, settle to steady state (spawns acked,
/// masks clear), then enable trace capture. Returns the resolved server-side
/// entity handles in a STABLE order (sorted), so `index` means the same entity in
/// both runs.
fn setup_and_settle(scenario: &mut Scenario) -> (ClientKey, Vec<TestEntity>) {
    let proto = protocol();
    scenario.server_start(ServerConfig::default(), proto.clone());
    let room_key = scenario.mutate(|mctx| mctx.server(|s| s.create_room().key()));
    let client_key: ClientKey = client_connect(
        scenario,
        &room_key,
        "client",
        Auth::new("user", "password"),
        client_config(),
        proto.clone(),
    );

    let mut entity_keys = Vec::new();
    for i in 0..NUM_ENTITIES {
        let (ek, _): (EntityKey, _) = scenario.mutate(|mctx| {
            mctx.server(|s| {
                s.spawn(|mut e| {
                    e.configure_replication(ReplicationConfig::public())
                        .insert_component(Position::new(i as f32, i as f32))
                        .enter_room(&room_key);
                })
            })
        });
        entity_keys.push(ek);
    }

    // Settle: client observes every entity, then a fixed number of extra ticks so
    // all spawns are acked and the per-user diff masks are clear. The fixed count
    // keeps both runs in lockstep.
    scenario.expect(|ctx| {
        entity_keys
            .iter()
            .all(|ek| ctx.client(client_key, |c| c.has_entity(ek)))
            .then_some(())
    });
    for _ in 0..20 {
        scenario.expect(|_| Some(()));
    }

    let mut server_entities = scenario.with_server_world_mut(|_s, world| {
        world.proxy().entities().into_iter().collect::<Vec<_>>()
    });
    // Stable order across both runs. TestEntity is Ord-comparable via its raw id.
    server_entities.sort_by_key(|e| format!("{e:?}"));
    assert_eq!(server_entities.len(), NUM_ENTITIES, "spawned entity count");

    scenario.enable_trace_capture();
    (client_key, server_entities)
}

/// Apply a case's mutations to the live server world.
fn apply_mutations(scenario: &mut Scenario, entities: &[TestEntity], case: &Case) {
    scenario.with_server_world_mut(|s, world| {
        for m in &case.mutations {
            if let Some(mut pos) = s
                .entity_mut(world.proxy_mut(), &entities[m.index])
                .component::<Position>()
            {
                if let Some(x) = m.set_x {
                    *pos.x = x;
                }
                if let Some(y) = m.set_y {
                    *pos.y = y;
                }
            }
        }
    });
}

/// Read the CURRENT full Position of each mutated entity and pack it into a
/// `SnapshotWorld` (the frozen world the pipelined transmit reads). The plan's
/// frozen `DiffMask` — not the snapshot — gates which properties actually
/// serialize, so packing the full component is correct for partial-mask cases.
fn snapshot_of(
    scenario: &mut Scenario,
    entities: &[TestEntity],
    case: &Case,
) -> SnapshotWorld<TestEntity> {
    let mut snap = SnapshotWorld::new();
    scenario.with_server_world_mut(|s, world| {
        for m in &case.mutations {
            let entity = entities[m.index];
            if let Some(pos) = s
                .entity_mut(world.proxy_mut(), &entity)
                .component::<Position>()
            {
                let boxed: Box<dyn Replicate> = Box::new(Position::new(*pos.x, *pos.y));
                snap.insert_component(entity, ComponentKind::of::<Position>(), boxed);
            }
        }
    });
    snap
}

/// Build the frozen `SnapshotWorld` via the **REAL core assembler** —
/// `SendStateView::build_needed_snapshot` (the registry-free, `WorldRefType`-based
/// G7 assembler) — instead of the hand-rolled `snapshot_of`.
///
/// This is the path the production pipelined `send` bracket uses (§2g item 4): it
/// reads the dirty-trim needed-set from naia's shared state and copies each
/// `(entity, kind)` out of the live world via the
/// `component_of_kind → copy_to_box → insert_component` chain. Discharges the
/// audit N4 G7 acceptance obligation: prove byte-identity while driving the
/// REAL assembler, not a hand-built snapshot.
fn snapshot_via_core_assembler(scenario: &mut Scenario) -> SnapshotWorld<TestEntity> {
    scenario.with_server_world_mut(|s, world| {
        let view = s.send_state_view();
        let world_ref = world.proxy();
        view.build_needed_snapshot(&world_ref)
    })
}

/// RESIDENT mode: mutate the live world, then one tick → `send_all_packets`
/// transmits synchronously against the live world.
fn run_resident(case: &Case) -> Vec<TracePacket> {
    let mut scenario = Scenario::new();
    let (_client_key, entities) = setup_and_settle(&mut scenario);
    apply_mutations(&mut scenario, &entities, case);
    let _ = scenario.expect_once(|_| ExpectResult::Passed(()));
    scenario.take_trace().packets
}

/// PIPELINED mode: mutate the live world, build the frozen plan + snapshot at the
/// freeze point, then one tick → `transmit_send_job` against the snapshot.
fn run_pipelined(case: &Case) -> Vec<TracePacket> {
    let mut scenario = Scenario::new();
    let (_client_key, entities) = setup_and_settle(&mut scenario);
    apply_mutations(&mut scenario, &entities, case);
    let snap = snapshot_of(&mut scenario, &entities, case);
    let plan = scenario.prepare_send_job();
    scenario.transmit_and_pump(snap, plan);
    scenario.take_trace().packets
}

/// G9pre strengthening (audit N4b): prove the freeze genuinely ISOLATES the
/// transmit from a concurrent next-tick live mutation — the actual reason the
/// one-tick lag + `SnapshotWorld` exist (the send worker transmits tick N's
/// frozen job while MAIN advances live state to tick N+1). After
/// `prepare_send_job` (freeze) we mutate the LIVE world to a DIFFERENT value;
/// the lagged transmit must emit the FROZEN value, byte-identical to a resident
/// send of that frozen value — i.e. the live advance must NOT leak onto the wire.
#[test]
fn g9pre_freeze_isolates_transmit_from_concurrent_mutation() {
    const FROZEN: (f32, f32) = (100.0, 200.0);
    const LIVE_AFTER: (f32, f32) = (999.0, 999.0);

    let frozen_case = Case {
        label: "frozen value",
        mutations: vec![EntityMutation {
            index: 0,
            set_x: Some(FROZEN.0),
            set_y: Some(FROZEN.1),
        }],
    };

    // Resident baseline: emits FROZEN.
    let resident = s2c(&run_resident(&frozen_case));

    // Pipelined WITH a concurrent post-freeze mutation to LIVE_AFTER.
    let pipelined = {
        let mut scenario = Scenario::new();
        let (_ck, entities) = setup_and_settle(&mut scenario);
        apply_mutations(&mut scenario, &entities, &frozen_case);
        let snap = snapshot_of(&mut scenario, &entities, &frozen_case); // captures FROZEN
        let plan = scenario.prepare_send_job(); // freeze point: mask captured + live mask cleared
                                                // Concurrent next-tick mutation must not appear on the wire this tick.
        let live_after = Case {
            label: "live after freeze",
            mutations: vec![EntityMutation {
                index: 0,
                set_x: Some(LIVE_AFTER.0),
                set_y: Some(LIVE_AFTER.1),
            }],
        };
        apply_mutations(&mut scenario, &entities, &live_after);
        scenario.transmit_and_pump(snap, plan);
        s2c(&scenario.take_trace().packets)
    };

    hexdump("RESIDENT(frozen)        ", &resident);
    hexdump("PIPELINED(frozen+live++)", &pipelined);

    assert_eq!(
        resident, pipelined,
        "G9pre N4b: the freeze must isolate the lagged transmit from a concurrent \
         post-freeze live mutation — the transmitted bytes must carry the FROZEN \
         value, not the advanced live value. A divergence means the snapshot/freeze \
         does not actually isolate the worker from main-thread state advance."
    );
}

/// G7 acceptance contract (audit N4): byte-identity while driving the REAL
/// core `WorldRefType` assembler (`SendStateView::build_needed_snapshot`) for the
/// frozen snapshot, across the full diff-mask case space. The original
/// `g9pre_resident_pipelined_byte_identity` hand-builds the snapshot; this test
/// proves the *production* assembler — the one whose skip-vs-panic is §2h M2 —
/// produces wire bytes identical to the resident baseline.
#[test]
fn g9pre_core_assembler_byte_identity() {
    let mut failures = Vec::new();

    for case in cases() {
        println!("\n══ [core-assembler] case: {} ══", case.label);
        let r = s2c(&run_resident(&case));

        let p = {
            let mut scenario = Scenario::new();
            let (_ck, entities) = setup_and_settle(&mut scenario);
            apply_mutations(&mut scenario, &entities, &case);
            let snap = snapshot_via_core_assembler(&mut scenario); // REAL assembler
            let plan = scenario.prepare_send_job();
            scenario.transmit_and_pump(snap, plan);
            s2c(&scenario.take_trace().packets)
        };

        hexdump("RESIDENT      ", &r);
        hexdump("CORE-ASSEMBLER", &p);

        if r.len() != p.len() {
            failures.push(format!(
                "[{}] packet-count divergence: resident={}, core-assembler={}",
                case.label,
                r.len(),
                p.len()
            ));
            continue;
        }
        let mismatches: Vec<usize> = r
            .iter()
            .zip(p.iter())
            .enumerate()
            .filter_map(|(i, (rb, pb))| (rb != pb).then_some(i))
            .collect();
        if !mismatches.is_empty() {
            failures.push(format!(
                "[{}] {} of {} packet(s) differ at indices {:?}",
                case.label,
                mismatches.len(),
                r.len(),
                mismatches
            ));
        } else {
            println!(
                "    ✓ byte-identical via core assembler ({} packet(s))",
                r.len()
            );
        }
    }

    assert!(
        failures.is_empty(),
        "G9pre N4: the REAL core WorldRefType assembler \
         (SendStateView::build_needed_snapshot) diverged from the resident \
         baseline. The production snapshot assembler does NOT reproduce resident \
         wire output. Findings:\n  {}",
        failures.join("\n  ")
    );
}

/// G7 acceptance contract (audit N4), strongest form: the REAL core assembler
/// PLUS a concurrent post-freeze live mutation. Builds the frozen snapshot via
/// `build_needed_snapshot`, freezes, then advances the LIVE world to a different
/// value before transmitting. The transmitted bytes must carry the FROZEN value
/// (byte-identical to a resident send of it) — proving the production assembler +
/// freeze together isolate the lagged transmit from main-thread state advance.
#[test]
fn g9pre_core_assembler_freeze_isolation() {
    const FROZEN: (f32, f32) = (100.0, 200.0);
    const LIVE_AFTER: (f32, f32) = (999.0, 999.0);

    let frozen_case = Case {
        label: "frozen value",
        mutations: vec![EntityMutation {
            index: 0,
            set_x: Some(FROZEN.0),
            set_y: Some(FROZEN.1),
        }],
    };

    let resident = s2c(&run_resident(&frozen_case));

    let pipelined = {
        let mut scenario = Scenario::new();
        let (_ck, entities) = setup_and_settle(&mut scenario);
        apply_mutations(&mut scenario, &entities, &frozen_case);
        let snap = snapshot_via_core_assembler(&mut scenario); // REAL assembler, captures FROZEN
        let plan = scenario.prepare_send_job();
        let live_after = Case {
            label: "live after freeze",
            mutations: vec![EntityMutation {
                index: 0,
                set_x: Some(LIVE_AFTER.0),
                set_y: Some(LIVE_AFTER.1),
            }],
        };
        apply_mutations(&mut scenario, &entities, &live_after);
        scenario.transmit_and_pump(snap, plan);
        s2c(&scenario.take_trace().packets)
    };

    hexdump("RESIDENT(frozen)            ", &resident);
    hexdump("CORE-ASSEMBLER(frozen+live++)", &pipelined);

    assert_eq!(
        resident, pipelined,
        "G9pre N4 (core assembler + freeze isolation): the production assembler \
         must copy the FROZEN component value into the snapshot at the freeze \
         point, and the freeze must isolate the transmit from the concurrent \
         post-freeze live mutation. A divergence means the assembled snapshot \
         either captured the wrong (post-mutation) value or the transmit read \
         live state."
    );
}

fn s2c(packets: &[TracePacket]) -> Vec<Vec<u8>> {
    packets
        .iter()
        .filter(|p| p.direction == TraceDirection::ServerToClient)
        .map(|p| p.bytes.clone())
        .collect()
}

fn hexdump(label: &str, packets: &[Vec<u8>]) {
    println!("    {label}: {} server→client packet(s)", packets.len());
    for (i, b) in packets.iter().enumerate() {
        let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
        println!("      [{i}] len={:>3}  {hex}", b.len());
    }
}

struct DirectScopeRun {
    hub: LocalTransportHub,
    server: Server<TestEntity>,
    server_world: TestWorld,
    client: Client<TestEntity>,
    client_world: TestWorld,
}

impl DirectScopeRun {
    fn new(mode: ServerMode) -> Self {
        TestClock::init(0);
        let server_addr: SocketAddr = "127.0.0.1:54590".parse().unwrap();
        let hub = LocalTransportHub::new(server_addr);

        let mut server = Server::new(mode, direct_server_config(), protocol());
        let server_socket = ServerSocket::new(LocalServerSocket::new(hub.clone()), None);
        server.listen(server_socket);

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
        let mut client = Client::new(client_config(), protocol());
        client.auth(Auth::new("user", "password"));
        client.connect(socket);

        Self {
            hub,
            server,
            server_world: TestWorld::default(),
            client,
            client_world: TestWorld::default(),
        }
    }

    fn tick_classic(&mut self) {
        TestClock::advance(16);
        let now = Instant::now();
        self.hub.process_time_queues();

        self.client.receive_all_packets();
        self.client
            .process_all_packets(self.client_world.proxy_mut(), &now);
        self.client.send_all_packets(self.client_world.proxy_mut());

        self.server.receive_all_packets();
        self.server
            .process_all_packets(self.server_world.proxy_mut(), &now);
        self.server.send_all_packets(self.server_world.proxy());
    }

    fn tick_bracket(&mut self) {
        TestClock::advance(16);
        let now = Instant::now();
        self.hub.process_time_queues();

        self.client.receive_all_packets();
        self.client
            .process_all_packets(self.client_world.proxy_mut(), &now);
        self.client.send_all_packets(self.client_world.proxy_mut());

        self.server.receive(self.server_world.proxy_mut());
        self.server.send(self.server_world.proxy());
    }

    fn connect(&mut self) -> naia_server::UserKey {
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

    fn setup_scoped_entity(&mut self, user_key: &naia_server::UserKey) -> TestEntity {
        let room_key = self.server.create_room().key();
        self.server.room_mut(&room_key).add_user(user_key);
        let entity = self.server_world.proxy_mut().spawn_entity();
        self.server
            .entity_mut(self.server_world.proxy_mut(), &entity)
            .enable_replication()
            .configure_replication(ReplicationConfig::public())
            .enter_room(&room_key);

        for _ in 0..32 {
            self.tick_classic();
        }
        entity
    }

    fn scope_trace(mut self) -> Vec<Vec<u8>> {
        let user_key = self.connect();
        let entity = self.setup_scoped_entity(&user_key);
        self.hub.enable_packet_recording();

        self.server.user_scope_mut(&user_key).exclude(&entity);
        self.tick_bracket();
        self.tick_bracket();
        self.server.user_scope_mut(&user_key).include(&entity);
        self.tick_bracket();
        self.tick_bracket();

        self.hub
            .take_recorded_packets()
            .into_iter()
            .filter_map(|(server_to_client, bytes)| server_to_client.then_some(bytes))
            .collect()
    }

    fn resource_trace(mut self) -> Vec<Vec<u8>> {
        let _user_key = self.connect();
        self.hub.enable_packet_recording();

        self.server
            .insert_resource(self.server_world.proxy_mut(), TestScore::new(7, 3), false)
            .expect("resource insert must succeed");
        self.tick_bracket();
        self.tick_bracket();
        assert!(self
            .server
            .remove_resource::<_, TestScore>(self.server_world.proxy_mut()));
        self.tick_bracket();
        self.tick_bracket();

        self.hub
            .take_recorded_packets()
            .into_iter()
            .filter_map(|(server_to_client, bytes)| server_to_client.then_some(bytes))
            .collect()
    }

    fn registration_trace(mut self) -> Vec<Vec<u8>> {
        let user_key = self.connect();
        let entity = self.setup_scoped_entity(&user_key);
        self.hub.enable_packet_recording();

        self.server
            .entity_mut(self.server_world.proxy_mut(), &entity)
            .configure_replication(ReplicationConfig::delegated());
        self.tick_bracket();
        self.tick_bracket();

        self.hub
            .take_recorded_packets()
            .into_iter()
            .filter_map(|(server_to_client, bytes)| server_to_client.then_some(bytes))
            .collect()
    }

    fn lifecycle_trace(mut self) -> Vec<Vec<u8>> {
        let user_key = self.connect();
        let entity = self.setup_scoped_entity(&user_key);
        self.hub.enable_packet_recording();

        self.server
            .entity_mut(self.server_world.proxy_mut(), &entity)
            .insert_component(TestScore::new(4, 2));
        self.tick_bracket();
        self.tick_bracket();

        let removed = self
            .server
            .entity_mut(self.server_world.proxy_mut(), &entity)
            .remove_component::<TestScore>();
        assert!(
            removed.is_some(),
            "TestScore must be present before removal"
        );
        self.tick_bracket();
        self.tick_bracket();

        self.server
            .entity_mut(self.server_world.proxy_mut(), &entity)
            .despawn();
        self.tick_bracket();
        self.tick_bracket();

        self.hub
            .take_recorded_packets()
            .into_iter()
            .filter_map(|(server_to_client, bytes)| server_to_client.then_some(bytes))
            .collect()
    }

    fn authority_trace(mut self) -> Vec<Vec<u8>> {
        let user_key = self.connect();
        let entity = self.setup_scoped_entity(&user_key);
        self.hub.enable_packet_recording();

        {
            let mut world = self.server_world.proxy_mut();
            assert!(self.server.enable_delegation(&mut world, &entity));
        }
        self.tick_bracket();
        self.tick_bracket();

        self.server
            .entity_give_authority(&user_key, &entity)
            .expect("give_authority must succeed for in-scope delegated entity");
        self.tick_bracket();
        self.tick_bracket();

        self.server
            .entity_take_authority(&entity)
            .expect("take_authority must succeed for delegated entity");
        self.tick_bracket();
        self.tick_bracket();

        self.server
            .entity_release_authority(None, &entity)
            .expect("release_authority must succeed for delegated entity");
        self.tick_bracket();
        self.tick_bracket();

        self.hub
            .take_recorded_packets()
            .into_iter()
            .filter_map(|(server_to_client, bytes)| server_to_client.then_some(bytes))
            .collect()
    }
}

#[test]
fn phase_c_d7_scope_ledger_resident_pipelined_oracle_byte_identity() {
    let resident = DirectScopeRun::new(ServerMode::Resident).scope_trace();
    let pipelined = DirectScopeRun::new(ServerMode::Pipelined).scope_trace();

    hexdump("D7 RESIDENT ", &resident);
    hexdump("D7 PIPELINED", &pipelined);

    assert_eq!(
        resident, pipelined,
        "Phase C D7: explicit user-scope exclude/include must emit byte-identical \
         server-to-client packets through resident and the real pipelined-oracle \
         WorldServer::send bracket"
    );
}

#[test]
fn phase_c_d2_resource_resident_pipelined_oracle_byte_identity() {
    let resident = DirectScopeRun::new(ServerMode::Resident).resource_trace();
    let pipelined = DirectScopeRun::new(ServerMode::Pipelined).resource_trace();

    hexdump("D2 RESIDENT ", &resident);
    hexdump("D2 PIPELINED", &pipelined);

    assert_eq!(
        resident, pipelined,
        "Phase C D2: resource insert/remove must emit byte-identical \
         server-to-client packets through resident and the real pipelined-oracle \
         WorldServer::send bracket"
    );
}

#[test]
fn phase_c_d1_registration_resident_pipelined_oracle_byte_identity() {
    let resident = DirectScopeRun::new(ServerMode::Resident).registration_trace();
    let pipelined = DirectScopeRun::new(ServerMode::Pipelined).registration_trace();

    hexdump("D1 RESIDENT ", &resident);
    hexdump("D1 PIPELINED", &pipelined);

    assert!(
        !resident.is_empty(),
        "Phase C D1 test must exercise a wire-producing registration transition"
    );
    assert_eq!(
        resident, pipelined,
        "Phase C D1: replication-config registration must emit byte-identical \
         server-to-client packets through resident and the real pipelined-oracle \
         WorldServer::send bracket"
    );
}

#[test]
fn phase_c_d3_lifecycle_resident_pipelined_oracle_byte_identity() {
    let resident = DirectScopeRun::new(ServerMode::Resident).lifecycle_trace();
    let pipelined = DirectScopeRun::new(ServerMode::Pipelined).lifecycle_trace();

    hexdump("D3 RESIDENT ", &resident);
    hexdump("D3 PIPELINED", &pipelined);

    assert!(
        !resident.is_empty(),
        "Phase C D3 test must exercise wire-producing lifecycle transitions"
    );
    assert_eq!(
        resident, pipelined,
        "Phase C D3: component insert/remove/despawn must emit byte-identical \
         server-to-client packets through resident and the real pipelined-oracle \
         WorldServer::send bracket"
    );
}

#[test]
fn phase_c_d4_authority_resident_pipelined_oracle_byte_identity() {
    let resident = DirectScopeRun::new(ServerMode::Resident).authority_trace();
    let pipelined = DirectScopeRun::new(ServerMode::Pipelined).authority_trace();

    hexdump("D4 RESIDENT ", &resident);
    hexdump("D4 PIPELINED", &pipelined);

    assert!(
        !resident.is_empty(),
        "Phase C D4 test must exercise wire-producing authority transitions"
    );
    assert_eq!(
        resident, pipelined,
        "Phase C D4: enable_delegation/give/take/release authority must emit \
         byte-identical server-to-client packets through resident and the real \
         pipelined-oracle WorldServer::send bracket"
    );
}

#[test]
fn g9pre_resident_pipelined_byte_identity() {
    let mut failures = Vec::new();

    for case in cases() {
        println!("\n══ case: {} ══", case.label);
        let r = s2c(&run_resident(&case));
        let p = s2c(&run_pipelined(&case));
        hexdump("RESIDENT ", &r);
        hexdump("PIPELINED", &p);

        if r.len() != p.len() {
            failures.push(format!(
                "[{}] packet-count divergence: resident={}, pipelined={}",
                case.label,
                r.len(),
                p.len()
            ));
            continue;
        }
        let mismatches: Vec<usize> = r
            .iter()
            .zip(p.iter())
            .enumerate()
            .filter_map(|(i, (rb, pb))| (rb != pb).then_some(i))
            .collect();
        if !mismatches.is_empty() {
            failures.push(format!(
                "[{}] {} of {} packet(s) differ byte-for-byte at indices {:?}",
                case.label,
                mismatches.len(),
                r.len(),
                mismatches
            ));
        } else {
            println!("    ✓ byte-identical ({} packet(s))", r.len());
        }
    }

    assert!(
        failures.is_empty(),
        "G9pre: Resident and Pipelined wire output diverged. The determinism/desync \
         oracle CANNOT be switched to Resident mode without changing the bytes the \
         harness sees. Findings:\n  {}",
        failures.join("\n  ")
    );
}
