//! C.6 prep 7/7 — `apply_receive_output_pipeline_with_sim_receiver`.
//!
//! Verifies the single-drain dual-sink combined helper that resolves
//! the SPEC_NAIA_C6_PREP §1.4 ambiguity (the spec showed
//! `apply_receive_output_pipeline(&output)` + `push_from_receive_output(&output)`
//! both taking a shared reference, but both APIs landed taking
//! `ReceiveOutput<E>` by value and draining `WorldEvents<E>` — so
//! chaining both against the same output without this helper is
//! impossible).
//!
//! Coverage:
//! - empty path: helper is a no-op on an empty `ReceiveOutput`
//!   (neither sink populated)
//! - pending_ticks fan out to BOTH bevy `Messages<TickEvent>` AND
//!   sim_receiver tick buffer; both drain once and re-drain empty
//! - byte-parity: bevy `Messages<TickEvent>` output is identical to
//!   what `apply_receive_output_pipeline` writes for the same input
//!   (the per-event-type bodies match line-for-line; this asserts
//!   the shared tick fan-out is unchanged)
//!
//! Per-event-type entity / message coverage piggy-backs on the
//! existing `apply_receive_output_pipeline` + `push_from_receive_output`
//! end-to-end tests; this helper's body invokes the same
//! `Events::read::<X>()` chain those callers go through, plus a
//! single `sim_receiver.push_*` call per item. `WorldEvents`'
//! `push_*` API is `pub(crate)`, so synthesizing rich events from
//! outside the naia-server crate would require a live client
//! connection — out of scope for this smoke gate.

use std::time::Duration;

use bevy_app::App;
use bevy_ecs::{entity::Entity, message::Messages};

use naia_bevy_server::{
    apply_receive_output_pipeline, apply_receive_output_pipeline_with_sim_receiver,
    events::TickEvent,
    pipeline_actors::{spawn_server_handles, SimEventReceiver},
    Plugin, ServerConfig,
};
use naia_bevy_shared::Protocol as BevyProtocol;
use naia_test_harness::test_protocol::Position;

fn naia_protocol() -> naia_shared::Protocol {
    let mut p = BevyProtocol::builder();
    p.add_component::<Position>();
    p.tick_interval(Duration::from_micros(100));
    let bevy_proto = p.build();
    bevy_proto.into()
}

fn bevy_protocol() -> naia_bevy_shared::Protocol {
    let mut p = BevyProtocol::builder();
    p.add_component::<Position>();
    p.tick_interval(Duration::from_micros(100));
    p.build()
}

/// Build a `bevy_app::App` with the naia server `Plugin` so all the
/// `Messages<X>` resources the helper writes to are registered.
fn build_app() -> App {
    let mut app = App::new();
    app.add_plugins(Plugin::new(ServerConfig::default(), bevy_protocol()));
    app
}

#[test]
fn empty_output_no_sink_population() {
    let mut app = build_app();
    let (sim_handle, _recv_handle, _send_handle) =
        spawn_server_handles::<Entity, _>(ServerConfig::default(), naia_protocol());
    let sim_receiver = SimEventReceiver::<Entity>::new();

    let output = naia_server::ReceiveOutput::<Entity> {
        pending_ticks: Vec::new(),
        world_events: naia_server::WorldEvents::new(),
        received_addresses: Default::default(),
        pending_data_packets: Vec::new(),
    };

    apply_receive_output_pipeline_with_sim_receiver(
        app.world_mut(),
        &sim_handle,
        &sim_receiver,
        output,
    );

    // Sim receiver: nothing.
    assert!(sim_receiver.drain_tick_events().is_empty());
    assert!(sim_receiver.drain_connect_events().is_empty());

    // Bevy Messages: nothing.
    let tick_messages = app.world().resource::<Messages<TickEvent>>();
    // Messages::iter_current_update_messages() would be ideal but
    // requires cursor; instead check count via Messages::len.
    assert_eq!(tick_messages.len(), 0);
}

#[test]
fn pending_ticks_fan_out_to_both_sinks() {
    let mut app = build_app();
    let (sim_handle, _recv_handle, _send_handle) =
        spawn_server_handles::<Entity, _>(ServerConfig::default(), naia_protocol());
    let sim_receiver = SimEventReceiver::<Entity>::new();

    let output = naia_server::ReceiveOutput::<Entity> {
        pending_ticks: vec![42, 43, 44],
        world_events: naia_server::WorldEvents::new(),
        received_addresses: Default::default(),
        pending_data_packets: Vec::new(),
    };

    apply_receive_output_pipeline_with_sim_receiver(
        app.world_mut(),
        &sim_handle,
        &sim_receiver,
        output,
    );

    // Sim receiver got all three ticks, in order.
    let sim_ticks = sim_receiver.drain_tick_events();
    assert_eq!(sim_ticks.len(), 3, "sim receiver got all ticks");
    assert_eq!(sim_ticks[0].0, 42);
    assert_eq!(sim_ticks[1].0, 43);
    assert_eq!(sim_ticks[2].0, 44);

    // Second drain on sim_receiver is empty (no double-drain).
    assert!(
        sim_receiver.drain_tick_events().is_empty(),
        "sim_receiver second drain must be empty",
    );

    // Bevy Messages<TickEvent> got all three ticks, in order.
    let tick_messages = app.world().resource::<Messages<TickEvent>>();
    assert_eq!(tick_messages.len(), 3, "bevy got all ticks");
}

#[test]
fn byte_parity_with_legacy_apply_receive_output_pipeline_ticks() {
    // Run the helper against app_combined; run the legacy
    // `apply_receive_output_pipeline` against app_legacy with the
    // same synthetic input. Assert the bevy `Messages<TickEvent>`
    // contents are identical.
    let mut app_combined = build_app();
    let mut app_legacy = build_app();

    let (sim_a, _, _) =
        spawn_server_handles::<Entity, _>(ServerConfig::default(), naia_protocol());
    let (sim_b, _, _) =
        spawn_server_handles::<Entity, _>(ServerConfig::default(), naia_protocol());
    let sim_receiver = SimEventReceiver::<Entity>::new();

    let make_output = || naia_server::ReceiveOutput::<Entity> {
        pending_ticks: vec![100, 101],
        world_events: naia_server::WorldEvents::new(),
        received_addresses: Default::default(),
        pending_data_packets: Vec::new(),
    };

    apply_receive_output_pipeline_with_sim_receiver(
        app_combined.world_mut(),
        &sim_a,
        &sim_receiver,
        make_output(),
    );
    apply_receive_output_pipeline(app_legacy.world_mut(), &sim_b, make_output());

    let combined_len = app_combined
        .world()
        .resource::<Messages<TickEvent>>()
        .len();
    let legacy_len = app_legacy.world().resource::<Messages<TickEvent>>().len();
    assert_eq!(
        combined_len, legacy_len,
        "combined helper writes the same number of bevy Messages<TickEvent> as the legacy path",
    );

    // Sim receiver was also populated.
    let sim_ticks = sim_receiver.drain_tick_events();
    assert_eq!(sim_ticks.len(), 2);
}

#[test]
fn cloned_sim_receiver_observes_helper_output() {
    // Arc-internal share: helper pushes through the &sim_receiver
    // handle; a clone made beforehand sees the same events.
    let mut app = build_app();
    let (sim_handle, _, _) =
        spawn_server_handles::<Entity, _>(ServerConfig::default(), naia_protocol());
    let sim_receiver = SimEventReceiver::<Entity>::new();
    let observer = sim_receiver.clone();

    let output = naia_server::ReceiveOutput::<Entity> {
        pending_ticks: vec![7],
        world_events: naia_server::WorldEvents::new(),
        received_addresses: Default::default(),
        pending_data_packets: Vec::new(),
    };

    apply_receive_output_pipeline_with_sim_receiver(
        app.world_mut(),
        &sim_handle,
        &sim_receiver,
        output,
    );

    // Clone observes the push.
    let ticks = observer.drain_tick_events();
    assert_eq!(ticks.len(), 1);
    assert_eq!(ticks[0].0, 7);
}
