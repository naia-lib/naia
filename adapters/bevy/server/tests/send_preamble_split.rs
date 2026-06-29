//! C.6 Addition 2 — `SendHandle::apply_pending_send_preamble()` split.
//!
//! Verifies the new split point on `SendHandle`:
//! - `apply_pending_send_preamble()` runs the preamble (room-change drain,
//!   outbound-flush, heartbeats, empty-acks) against `SendState` alone.
//! - The subsequent `send_all_packets` notices the flag and skips its
//!   inline preamble.
//! - Backward compat: callers that invoke `send_all_packets` directly
//!   (without first calling `apply_pending_send_preamble`) still get
//!   the preamble inline.
//!
//! Observable: room-change queue length. `room_destroy` pushes a
//! `RoomChange::RoomDestroyed` onto `shared.scope_change_queue`; the
//! preamble drains every `RoomChange::*` variant out of the queue.

use std::time::Duration;

use bevy_ecs::{entity::Entity, world::World};

use naia_bevy_server::{
    pipeline_actors::{run_with_world_server, spawn_server_handles},
    ServerConfig,
};
use naia_bevy_shared::{Protocol as BevyProtocol, WorldProxy};
use naia_test_harness::test_protocol::Position;

fn protocol() -> naia_shared::Protocol {
    let mut p = BevyProtocol::builder();
    p.register_component::<Position>();
    p.tick_interval(Duration::from_micros(100));
    let bevy_proto = p.build();
    bevy_proto.into()
}

fn handles_listening(
    addr: &str,
) -> (
    naia_server::pipeline_actors::CoordHandle<Entity>,
    naia_server::RecvHandle<Entity>,
    naia_server::SendHandle<Entity>,
) {
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

fn queue_len(sim_handle: &naia_server::pipeline_actors::CoordHandle<Entity>) -> usize {
    sim_handle.scope_change_queue_len()
}

#[test]
fn apply_pending_send_preamble_drains_room_change_queue() {
    let (mut sim_handle, _recv, mut send) = handles_listening(next_addr());

    // Push a RoomChange onto the queue: create + destroy an empty room.
    let room_key = sim_handle.create_room();
    let destroyed = sim_handle.room_destroy(&room_key);
    assert!(destroyed);
    assert!(
        queue_len(&sim_handle) >= 1,
        "room_destroy must push at least one ScopeChange",
    );

    // Run the new preamble split point on SendHandle.
    send.apply_pending_send_preamble();

    // RoomChange variants are drained (nothing else on this queue
    // should remain since no scope toggles were pushed).
    assert_eq!(
        queue_len(&sim_handle),
        0,
        "apply_pending_send_preamble must drain RoomChange variants",
    );
}

#[test]
fn second_call_to_send_all_packets_does_not_double_run_preamble() {
    let (mut sim_handle, _recv, mut send) = handles_listening(next_addr());

    let k = sim_handle.create_room();
    let _ = sim_handle.room_destroy(&k);
    assert!(queue_len(&sim_handle) >= 1);

    // Explicit preamble: drains the queue, sets flag.
    send.apply_pending_send_preamble();
    assert_eq!(queue_len(&sim_handle), 0);

    // Push another RoomChange — this lands AFTER the preamble ran.
    let k2 = sim_handle.create_room();
    let _ = sim_handle.room_destroy(&k2);
    assert!(
        queue_len(&sim_handle) >= 1,
        "second push must land on queue"
    );

    // Empty world for send_all_packets.
    let world = World::new();
    send.send_all_packets(world.proxy());

    // send_all_packets must SKIP its inline preamble (flag was true),
    // so the post-preamble RoomChange is STILL in the queue.
    assert!(
        queue_len(&sim_handle) >= 1,
        "send_all_packets must SKIP preamble when apply_pending_send_preamble already ran",
    );

    // Second send_all_packets WITHOUT explicit preamble — flag was
    // reset, so it runs the preamble inline.
    let world2 = World::new();
    send.send_all_packets(world2.proxy());
    assert_eq!(
        queue_len(&sim_handle),
        0,
        "second send_all_packets (no explicit preamble) must run inline preamble",
    );
}

#[test]
fn backward_compat_send_all_packets_alone_still_drains_queue() {
    let (mut sim_handle, _recv, mut send) = handles_listening(next_addr());

    let k = sim_handle.create_room();
    let _ = sim_handle.room_destroy(&k);
    assert!(queue_len(&sim_handle) >= 1);

    // Call send_all_packets directly — must drain queue inline (no
    // explicit preamble called).
    let world = World::new();
    send.send_all_packets(world.proxy());

    assert_eq!(
        queue_len(&sim_handle),
        0,
        "backward compat: send_all_packets alone must drain the room queue",
    );
}

// Per-test address allocator.
use std::sync::atomic::{AtomicUsize, Ordering};
static PORT_COUNTER: AtomicUsize = AtomicUsize::new(59000);
fn next_addr() -> &'static str {
    let port = PORT_COUNTER.fetch_add(1, Ordering::SeqCst);
    Box::leak(format!("127.0.0.1:{port}").into_boxed_str())
}
