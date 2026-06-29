//! C.6 prep #6 — `SendHandle::apply_pending_scope_changes` drains the
//! entity-scope variants (`EntityEnteredRoom`, `UserEnteredRoom`,
//! `UserLeftRoom`, `ScopeToggled`) from `scope_change_queue` and fans
//! them out to user connections.
//!
//! Companion to `send_preamble_split.rs` (which exercises the
//! `RoomChange`-only drain). Together the two methods restore the full
//! body of the legacy `WorldServer::run_send_preamble` →
//! `update_entity_scopes` → `drain_scope_change_queue` chain for
//! pipeline-mode callers that hold a `SendHandle` directly.
//!
//! Observable surface: `scope_change_queue.len()` (via
//! `SimHandle::scope_change_queue_len`). The legacy variants are
//! pushed alongside the new `RoomChange` payloads by
//! `room_store::add_user` / `add_entity`; the preamble drains only
//! `RoomChange`, leaving the legacy entity-scope variants behind. The
//! new method consumes them. Full spawn-into-user wire fanout is
//! covered by the namako integration test (the wire-byte parity canary).

use std::time::Duration;

use bevy_ecs::{entity::Entity, world::World};

use naia_bevy_server::{
    pipeline_actors::{run_with_world_server, spawn_server_handles, SimHandle},
    ServerConfig,
};
use naia_bevy_shared::{Protocol as BevyProtocol, WorldProxy};
use naia_server::{RecvHandle, SendHandle};
use naia_test_harness::test_protocol::Position;

fn protocol() -> naia_shared::Protocol {
    let mut p = BevyProtocol::builder();
    p.register_component::<Position>();
    p.tick_interval(Duration::from_micros(100));
    let bevy_proto = p.build();
    bevy_proto.into()
}

fn handles_listening(addr: &str) -> (SimHandle<Entity>, RecvHandle<Entity>, SendHandle<Entity>) {
    use naia_server::transport::local::{LocalServerSocket, LocalTransportHub, Socket};

    let (sim_handle, recv, send) =
        spawn_server_handles::<Entity, _>(ServerConfig::default(), protocol()).take_handles();

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

/// Make a real bevy entity that we can register against naia's
/// `global_entity_map` via `WorldServer::spawn_entity`. The room_*
/// methods on `SimHandle` go through the
/// `shared.global_entity_map.entity_to_global_entity(world_entity)`
/// lookup, which requires the entity to be registered.
fn spawn_replicating_entity(
    sim_handle: SimHandle<Entity>,
    recv: RecvHandle<Entity>,
    send: SendHandle<Entity>,
    bevy_world: &mut World,
) -> (
    SimHandle<Entity>,
    RecvHandle<Entity>,
    SendHandle<Entity>,
    Entity,
) {
    let entity = bevy_world.spawn(Position::new(1.0, 2.0)).id();
    let (sim_handle, recv, send, ()) = run_with_world_server(sim_handle, recv, send, |ws| {
        // Register the bevy entity with naia (server-owned). This
        // makes `room_add_entity` / `room_add_user` resolve
        // `world_entity → GlobalEntity` and push the expected
        // ScopeChange + RoomChange pair.
        ws.enable_entity_replication(&entity);
    });
    (sim_handle, recv, send, entity)
}

#[test]
fn apply_pending_scope_changes_drains_entity_enter_room_variant() {
    let (sim_handle, recv, send) = handles_listening(next_addr());
    let mut bevy_world = World::new();
    let (mut sim_handle, _recv, mut send, entity) =
        spawn_replicating_entity(sim_handle, recv, send, &mut bevy_world);

    // create_room + room_add_entity pushes BOTH:
    //   - ScopeChange::EntityEnteredRoom(global_entity, room_key)  ← legacy
    //   - ScopeChange::RoomChange(RoomChange::EntityAdded { .. })  ← C.6-prep #2
    let room_key = sim_handle.create_room();
    let pre_add_len = queue_len(&sim_handle);
    sim_handle.room_add_entity(&room_key, &entity);
    assert!(
        queue_len(&sim_handle) >= pre_add_len + 2,
        "room_add_entity must push at least one legacy + one RoomChange (queue grew {} → {})",
        pre_add_len,
        queue_len(&sim_handle),
    );

    // Preamble drains only the RoomChange variants — the legacy
    // EntityEnteredRoom variant survives.
    send.apply_pending_send_preamble();
    assert!(
        queue_len(&sim_handle) >= 1,
        "preamble must leave the legacy entity-scope variant in the queue (len={})",
        queue_len(&sim_handle),
    );

    // The new method drains the remaining entity-scope variants.
    send.apply_pending_scope_changes(&bevy_world.proxy());
    assert_eq!(
        queue_len(&sim_handle),
        0,
        "apply_pending_scope_changes must drain all remaining entity-scope variants",
    );
}

#[test]
fn send_all_packets_auto_calls_scope_changes_drain_when_flag_unset() {
    // Backward-compat path: caller invokes neither preamble nor
    // scope-changes drain. send_all_packets auto-calls both inline.
    let (sim_handle, recv, send) = handles_listening(next_addr());
    let mut bevy_world = World::new();
    let (mut sim_handle, _recv, mut send, entity) =
        spawn_replicating_entity(sim_handle, recv, send, &mut bevy_world);

    let room_key = sim_handle.create_room();
    sim_handle.room_add_entity(&room_key, &entity);
    assert!(queue_len(&sim_handle) >= 2);

    // Call send_all_packets directly (no preamble, no scope drain).
    send.send_all_packets(bevy_world.proxy());

    assert_eq!(
        queue_len(&sim_handle),
        0,
        "send_all_packets must auto-drain entity-scope variants when the per-tick flag is unset",
    );
}

#[test]
fn apply_pending_scope_changes_is_idempotent_within_one_tick() {
    let (sim_handle, recv, send) = handles_listening(next_addr());
    let mut bevy_world = World::new();
    let (mut sim_handle, _recv, mut send, entity) =
        spawn_replicating_entity(sim_handle, recv, send, &mut bevy_world);

    let room_key = sim_handle.create_room();
    sim_handle.room_add_entity(&room_key, &entity);
    send.apply_pending_send_preamble();
    send.apply_pending_scope_changes(&bevy_world.proxy());
    assert_eq!(queue_len(&sim_handle), 0);

    // Second call — the per-tick flag is now set, and the queue is
    // empty. Must be a no-op (no panic, no double-drain side effects).
    send.apply_pending_scope_changes(&bevy_world.proxy());
    assert_eq!(queue_len(&sim_handle), 0);

    // send_all_packets must SKIP its auto-call because the flag is set.
    // Push something onto the queue AFTER the explicit call; if
    // send_all_packets's auto-call still fired, the new entry would be
    // drained too.
    let k2 = sim_handle.create_room();
    sim_handle.room_add_entity(&k2, &entity);
    let post_push_len = queue_len(&sim_handle);
    assert!(post_push_len >= 2, "post-drain push must land on queue");

    // Empty-snap send_all_packets — flag set → auto-call SKIPPED →
    // entity-scope variants survive.
    let empty_world = World::new();
    send.send_all_packets(empty_world.proxy());
    // The preamble auto-call drains the new RoomChange; the
    // scope-changes auto-call is SKIPPED (flag was set). So the
    // legacy variant survives.
    assert!(
        queue_len(&sim_handle) >= 1,
        "send_all_packets must SKIP scope-changes auto-call when the flag is set (len={})",
        queue_len(&sim_handle),
    );

    // Next tick's send_all_packets resets the flag and drains.
    let empty_world2 = World::new();
    send.send_all_packets(empty_world2.proxy());
    assert_eq!(
        queue_len(&sim_handle),
        0,
        "next tick's send_all_packets must drain everything",
    );
}

#[test]
fn user_left_room_variant_is_drained() {
    // `room_remove_user` pushes a legacy `UserLeftRoom` variant
    // alongside the `RoomChange::UserRemoved`. The new method must
    // consume the legacy variant.
    let (sim_handle, recv, send) = handles_listening(next_addr());
    let mut bevy_world = World::new();
    let (mut sim_handle, _recv, mut send, _entity) =
        spawn_replicating_entity(sim_handle, recv, send, &mut bevy_world);

    let room_key = sim_handle.create_room();
    // Pretend a user joined and then left. We can't easily set up a
    // real UserKey without a full handshake, so we observe via the
    // queue: the room ops still push the variants.
    use naia_server::UserKey;
    use naia_shared::BigMapKey;
    let synthetic_user_key = UserKey::from_u64(1);
    sim_handle.room_add_user(&room_key, &synthetic_user_key);
    sim_handle.room_remove_user(&room_key, &synthetic_user_key);
    assert!(
        queue_len(&sim_handle) >= 4,
        "add_user + remove_user must push 2 legacy + 2 RoomChange variants",
    );

    send.apply_pending_send_preamble();
    send.apply_pending_scope_changes(&bevy_world.proxy());
    assert_eq!(
        queue_len(&sim_handle),
        0,
        "all entity-scope variants must be drained",
    );
}

// Per-test address allocator.
use std::sync::atomic::{AtomicUsize, Ordering};
static PORT_COUNTER: AtomicUsize = AtomicUsize::new(59500);
fn next_addr() -> &'static str {
    let port = PORT_COUNTER.fetch_add(1, Ordering::SeqCst);
    Box::leak(format!("127.0.0.1:{port}").into_boxed_str())
}
