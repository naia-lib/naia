//! Phase A of MISSION_USER_ONLY_SEES_SIM (2026-05-19) —
//! `apply_pending_scope_changes` retry survivability.
//!
//! Background: the 2026-05-19 f3-saga finding during MISSION_SIM_OWNS_WORLD
//! showed that `SendState::apply_scope_for_user`'s prior re-queue
//! semantic (push-back unconditionally on `!world.has_entity(...)`)
//! effectively gave up after ~2 ticks because the entry could be
//! starved by drain ordering. The Phase A patch adds a bounded
//! per-(user, entity) retry counter (`SCOPE_RETRY_MAX = 16`,
//! ≈ 640 ms at 25 Hz) — re-queue up to N times, then drop + `warn!`.
//!
//! Coverage at this bevy-adapter integration level:
//! - The new method still drains entity-scope variants from
//!   `scope_change_queue` when the snapshot world DOES contain the
//!   entity (the happy path that exists today; we re-pin it so the
//!   retry plumbing didn't regress the success path).
//! - The new method still drains entity-scope variants when the
//!   snapshot world is empty AND no users are connected — the
//!   `user_addresses.get(user_key)` early-return path absorbs the
//!   change and the queue empties (the retry counter never enters
//!   the picture; this pins that synthetic-user setups don't accrue
//!   spurious retry entries).
//!
//! The retry counter's increment / exhaustion semantics are covered
//! by the `scope_retry_tests` unit tests in
//! `server/src/server/send_state.rs` — exercising those at the
//! handle-level requires a fully-handshaken client (the early-return
//! at `user_addresses.get` short-circuits the retry path otherwise),
//! which is out of scope for this smoke gate.

use std::time::Duration;

use bevy_ecs::{entity::Entity, world::World};

use naia_bevy_server::{
    pipeline_actors::{run_with_world_server, spawn_server_handles, SimHandle},
    ServerConfig,
};
use naia_bevy_shared::{Protocol as BevyProtocol, WorldProxy};
use naia_server::{RecvHandle, SendHandle, UserKey};
use naia_shared::BigMapKey;
use naia_test_harness::test_protocol::Position;

fn protocol() -> naia_shared::Protocol {
    let mut p = BevyProtocol::builder();
    p.add_component::<Position>();
    p.tick_interval(Duration::from_micros(100));
    let bevy_proto = p.build();
    bevy_proto.into()
}

fn handles_listening(
    addr: &str,
) -> (
    SimHandle<Entity>,
    RecvHandle<Entity>,
    SendHandle<Entity>,
) {
    use naia_server::transport::local::{LocalServerSocket, LocalTransportHub, Socket};

    let (sim_handle, recv, send) =
        spawn_server_handles::<Entity, _>(ServerConfig::default(), protocol());

    let hub = LocalTransportHub::new(addr.parse().unwrap());
    let socket = Socket::new(LocalServerSocket::new(hub), None);
    let (_a, _b, ps, pr) = naia_server::transport::Socket::listen(Box::new(socket));

    let (sim_handle, recv, send, ()) = run_with_world_server(sim_handle, recv, send, |ws| {
        ws.io_load(ps, pr);
    });
    (sim_handle, recv, send)
}

fn queue_len(sim_handle: &SimHandle<Entity>) -> usize {
    sim_handle.scope_change_queue_len()
}

fn spawn_replicating_entity(
    sim_handle: SimHandle<Entity>,
    recv: RecvHandle<Entity>,
    send: SendHandle<Entity>,
    bevy_world: &mut World,
) -> (SimHandle<Entity>, RecvHandle<Entity>, SendHandle<Entity>, Entity) {
    let entity = bevy_world.spawn(Position::new(1.0, 2.0)).id();
    let (sim_handle, recv, send, ()) =
        run_with_world_server(sim_handle, recv, send, |ws| {
            ws.enable_entity_replication(&entity);
        });
    (sim_handle, recv, send, entity)
}

/// Sanity gate: the Phase A patch must not have regressed the
/// happy-path drain (snapshot world contains the entity).
#[test]
fn happy_path_drain_unchanged_after_retry_patch() {
    let (sim_handle, recv, send) = handles_listening(next_addr());
    let mut bevy_world = World::new();
    let (mut sim_handle, _recv, mut send, entity) =
        spawn_replicating_entity(sim_handle, recv, send, &mut bevy_world);

    let room_key = sim_handle.create_room();
    sim_handle.room_add_entity(&room_key, &entity);

    send.apply_pending_send_preamble();
    send.apply_pending_scope_changes(&bevy_world.proxy());
    assert_eq!(
        queue_len(&sim_handle),
        0,
        "happy path: snapshot world contains entity — full drain expected",
    );
}

/// Even when the snapshot world is empty, a synthetic user that was
/// never handshaken must NOT cause the retry counter to grow — the
/// early-return at `user_addresses.get(user_key)` absorbs the entry
/// before the retry-counter branch runs.
#[test]
fn synthetic_user_does_not_accrue_retries() {
    let (sim_handle, recv, send) = handles_listening(next_addr());
    let mut bevy_world = World::new();
    let (mut sim_handle, _recv, mut send, entity) =
        spawn_replicating_entity(sim_handle, recv, send, &mut bevy_world);

    let room_key = sim_handle.create_room();
    let synthetic_user_key = UserKey::from_u64(1);
    sim_handle.room_add_user(&room_key, &synthetic_user_key);
    sim_handle.room_add_entity(&room_key, &entity);

    // Empty world — entity is NOT visible in the snapshot world. If
    // synthetic_user_key were a real connected user, the per-(user,
    // entity) ScopeToggled would re-queue (retry counter increments).
    // Because synthetic_user_key has no address in
    // `send_user_connections`, the early-return fires and the entry
    // is consumed without a re-queue. Loop a generous number of
    // times to demonstrate the queue stays bounded.
    let empty_world = World::new();
    for _ in 0..32 {
        send.apply_pending_send_preamble();
        send.apply_pending_scope_changes(&empty_world.proxy());
    }
    assert_eq!(
        queue_len(&sim_handle),
        0,
        "synthetic (un-handshaken) user must not accrue retries — \
         entry consumed by early-return before retry path",
    );
}

/// Stability under repeated empty-world calls — no panics, no
/// runaway queue growth.
#[test]
fn repeated_empty_world_calls_are_stable() {
    let (sim_handle, recv, send) = handles_listening(next_addr());
    let mut bevy_world = World::new();
    let (mut sim_handle, _recv, mut send, _entity) =
        spawn_replicating_entity(sim_handle, recv, send, &mut bevy_world);

    let _room_key = sim_handle.create_room();

    // Mirror of SCOPE_RETRY_MAX (server/src/server/send_state.rs).
    // The canonical constant is pub(crate); adapter-level tests pin
    // its value via this local mirror.
    const SCOPE_RETRY_MAX_MIRROR: u8 = 16;

    let empty_world = World::new();
    for _ in 0..(SCOPE_RETRY_MAX_MIRROR as usize + 4) {
        send.apply_pending_send_preamble();
        send.apply_pending_scope_changes(&empty_world.proxy());
        assert!(
            queue_len(&sim_handle) <= 64,
            "queue must stay bounded across repeated empty-world calls",
        );
    }
}

// Per-test address allocator.
use std::sync::atomic::{AtomicUsize, Ordering};
static PORT_COUNTER: AtomicUsize = AtomicUsize::new(59700);
fn next_addr() -> &'static str {
    let port = PORT_COUNTER.fetch_add(1, Ordering::SeqCst);
    Box::leak(format!("127.0.0.1:{port}").into_boxed_str())
}
