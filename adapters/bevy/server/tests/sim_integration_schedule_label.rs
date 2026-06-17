//! C.6 Addition 1 — schedule-label knob on `Plugin::sim_integration`.
//!
//! Verifies that `Plugin::sim_integration_with_schedule(...)` registers
//! the per-`Replicate` `on_component_added::<R>` / `on_component_removed::<R>`
//! change-tracking systems in the supplied custom schedule rather than
//! the canonical `Update`.
//!
//! Cyberlith Sim's gameplay loop drives a non-`Update` schedule
//! (`SimMain`). If the change-tracking systems registered under
//! `Update`, naia would never see Sim-side component inserts because
//! Sim never invokes `Update`.
//!
//! Strategy: install the plugin with a `CustomSchedule`, spawn a
//! Position-bearing entity with `HostOwned`, then:
//!   1. Running only `Update` produces NO `HostSyncEvent`s.
//!   2. Running `CustomSchedule` produces the expected `Insert`
//!      event for the new component.

use std::time::Duration;

use bevy_app::App;
use bevy_ecs::{
    message::Messages,
    schedule::{Schedule, ScheduleLabel},
};

use naia_bevy_server::{Plugin as ServerPlugin, ServerConfig};
use naia_bevy_shared::{HostOwned, HostSyncEvent, Protocol as BevyProtocol};

use naia_test_harness::test_protocol::Position;

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct CustomSchedule;

fn protocol() -> BevyProtocol {
    let mut p = BevyProtocol::builder();
    p.register_component::<Position>();
    p.tick_interval(Duration::from_micros(100));
    p.build()
}

/// Build a bevy App with `Plugin::sim_integration_with_schedule(CustomSchedule)`
/// installed. Pre-registers the `CustomSchedule` so plugin's
/// `app.add_systems(schedule, ...)` call succeeds without auto-creating
/// a schedule that no other code runs.
fn build_app_with_custom_schedule() -> App {
    let mut app = App::new();
    app.add_schedule(Schedule::new(CustomSchedule));
    app.add_plugins(ServerPlugin::sim_integration_with_schedule(
        ServerConfig::default(),
        protocol(),
        CustomSchedule,
    ));
    app
}

#[doc(hidden)]
#[derive(Clone, Copy)]
struct Singleton;

#[test]
fn sim_integration_with_schedule_plugin_builds() {
    let _app = build_app_with_custom_schedule();
}

#[test]
fn update_alone_yields_no_host_sync_events_under_custom_schedule() {
    let mut app = build_app_with_custom_schedule();
    app.world_mut()
        .spawn((HostOwned::new::<Singleton>(), Position::new(1.0, 2.0)));

    // Run only `Update`. Because the per-Replicate change-detection
    // systems were registered under `CustomSchedule`, `Update` alone
    // must produce no `HostSyncEvent`s.
    app.update();

    let events = app
        .world()
        .get_resource::<Messages<HostSyncEvent>>()
        .expect("HostSyncEvent Messages resource present");
    let count = events.iter_current_update_messages().count();
    assert_eq!(
        count, 0,
        "Update alone must NOT fire HostSyncEvent when change-tracking is bound to CustomSchedule (got {count} events)",
    );
}

#[test]
fn custom_schedule_run_yields_insert_host_sync_event() {
    let mut app = build_app_with_custom_schedule();
    app.world_mut()
        .spawn((HostOwned::new::<Singleton>(), Position::new(3.0, 4.0)));

    // Running CustomSchedule must fire the on_component_added::<Position>
    // system, which writes a HostSyncEvent::Insert into the Messages
    // buffer.
    app.world_mut().run_schedule(CustomSchedule);

    let events = app
        .world()
        .get_resource::<Messages<HostSyncEvent>>()
        .expect("HostSyncEvent Messages resource present");
    let inserts: Vec<_> = events
        .iter_current_update_messages()
        .filter(|ev| matches!(ev, HostSyncEvent::Insert(..)))
        .collect();
    assert!(
        !inserts.is_empty(),
        "running CustomSchedule must fire HostSyncEvent::Insert for Position; got {} events total",
        events.iter_current_update_messages().count(),
    );
}

/// Sanity: the default `sim_integration` constructor still registers
/// under `Update` (backward compatibility).
#[test]
fn default_sim_integration_still_registers_under_update() {
    let mut app = App::new();
    app.add_plugins(ServerPlugin::sim_integration(
        ServerConfig::default(),
        protocol(),
    ));
    app.world_mut()
        .spawn((HostOwned::new::<Singleton>(), Position::new(5.0, 6.0)));
    app.update();

    let events = app
        .world()
        .get_resource::<Messages<HostSyncEvent>>()
        .expect("HostSyncEvent Messages resource present");
    let inserts: Vec<_> = events
        .iter_current_update_messages()
        .filter(|ev| matches!(ev, HostSyncEvent::Insert(..)))
        .collect();
    assert!(
        !inserts.is_empty(),
        "default sim_integration → Update must still fire HostSyncEvent::Insert",
    );
}
