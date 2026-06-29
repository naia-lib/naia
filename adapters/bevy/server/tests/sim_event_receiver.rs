//! C.6 Addition 4 — `EventReceiver<E>` facade.
//!
//! Verifies that `push_from_receive_output` fans a `ReceiveOutput` into
//! the typed per-event-type drainers.
//!
//! Smoke-level: builds a `ReceiveOutput` with a synthetic tick and an
//! empty `WorldEvents`, pushes it through the receiver, and asserts
//! `drain_tick_events` yields the tick.
//!
//! Full end-to-end coverage (real connect → spawn → message) is
//! provided by the existing `apply_receive_output_pipeline` tests in
//! this crate's test directory; the receiver's body shares the same
//! `Events::read` calls byte-for-byte against the same `WorldEvents`,
//! so once the smoke path is verified, semantic parity holds.

use std::time::Duration;

use bevy_ecs::entity::Entity;

use naia_bevy_server::{
    pipeline_actors::{spawn_server_handles, EventReceiver},
    ServerConfig,
};
use naia_bevy_shared::Protocol as BevyProtocol;
use naia_test_harness::test_protocol::Position;

fn protocol() -> naia_shared::Protocol {
    let mut p = BevyProtocol::builder();
    p.register_component::<Position>();
    p.tick_interval(Duration::from_micros(100));
    let bevy_proto = p.build();
    bevy_proto.into()
}

#[test]
fn fresh_receiver_drains_empty() {
    let recv = EventReceiver::<Entity>::new();
    assert!(recv.drain_connect_events().is_empty());
    assert!(recv.drain_disconnect_events().is_empty());
    assert!(recv.drain_error_events().is_empty());
    assert!(recv.drain_tick_events().is_empty());
    assert!(recv.drain_spawn_entity_events().is_empty());
    assert!(recv.drain_despawn_entity_events().is_empty());
    assert!(recv.drain_publish_entity_events().is_empty());
    assert!(recv.drain_unpublish_entity_events().is_empty());
}

#[test]
fn push_pending_ticks_lands_in_tick_drainer() {
    let (sim_handle, _recv_handle, _send_handle) =
        spawn_server_handles::<Entity, _>(ServerConfig::default(), protocol()).take_handles();

    let receiver = EventReceiver::<Entity>::new();

    // Synthesize a ReceiveOutput<Entity> with two ticks.
    let output = naia_server::ReceiveOutput::<Entity> {
        pending_ticks: vec![10, 11],
        world_events: naia_server::WorldEvents::new(),
        received_addresses: Default::default(),
        pending_data_packets: Vec::new(),
    };

    receiver.push_from_receive_output(&sim_handle, output);

    let ticks = receiver.drain_tick_events();
    assert_eq!(ticks.len(), 2, "two pending ticks must fan out");
    assert_eq!(ticks[0].0, 10);
    assert_eq!(ticks[1].0, 11);

    // Second drain yields empty.
    let ticks2 = receiver.drain_tick_events();
    assert!(ticks2.is_empty(), "drained twice should be empty");
}

#[test]
fn cloned_receiver_shares_state() {
    // Arc-internal: clones share the same backing storage.
    let (sim_handle, _recv_handle, _send_handle) =
        spawn_server_handles::<Entity, _>(ServerConfig::default(), protocol()).take_handles();

    let receiver = EventReceiver::<Entity>::new();
    let clone = receiver.clone();

    let output = naia_server::ReceiveOutput::<Entity> {
        pending_ticks: vec![99],
        world_events: naia_server::WorldEvents::new(),
        received_addresses: Default::default(),
        pending_data_packets: Vec::new(),
    };
    receiver.push_from_receive_output(&sim_handle, output);

    // Clone sees the same event.
    let ticks = clone.drain_tick_events();
    assert_eq!(ticks.len(), 1);
    assert_eq!(ticks[0].0, 99);

    // Original is now empty (drain took the buffer).
    assert!(receiver.drain_tick_events().is_empty());
}
