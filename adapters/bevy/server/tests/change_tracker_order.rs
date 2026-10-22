//! R1+R2 for deterministic change-tracker registration order (Usher 105946).
//!
//! `WorldData` registers every component kind's `on_component_added` /
//! `on_component_removed` pair in protocol registration order, chained
//! through per-kind [`HostSyncKindSlot`]s with `on_despawn` last. These
//! tests pin that contract at the lowest rungs:
//!
//! - R1: two independently built apps from the same protocol register the
//!   identical tracker order, and that order is protocol registration order.
//!   Observed through the `HostSyncEvent::Insert` sequence one tick emits
//!   for a single entity carrying every component: sequential chained
//!   systems write events in execution order.
//! - R2: a Bevy schedule-ambiguity check over the naia-registered systems
//!   reports 0 pairs among them, on both the `Update` path and a custom
//!   (SimMain-like) schedule path.

use std::collections::HashMap;
use std::time::Duration;

use bevy_app::{App, Update};
use bevy_ecs::{
    message::Messages,
    schedule::{Schedule, ScheduleLabel, Schedules},
};

use naia_bevy_server::{Plugin as ServerPlugin, ServerConfig};
use naia_bevy_shared::{ComponentKind, HostOwned, HostSyncEvent, Protocol as BevyProtocol};

use naia_test_harness::test_protocol::{ImmutableLabel, ImmutableSeed, Position, Velocity};

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct CustomSchedule;

fn protocol() -> BevyProtocol {
    let mut p = BevyProtocol::builder();
    p.register_component::<Position>();
    p.register_component::<Velocity>();
    p.register_component::<ImmutableLabel>();
    p.register_component::<ImmutableSeed>();
    p.tick_interval(Duration::from_micros(100));
    p.build()
}

fn expected_kind_order() -> Vec<ComponentKind> {
    vec![
        ComponentKind::of::<Position>(),
        ComponentKind::of::<Velocity>(),
        ComponentKind::of::<ImmutableLabel>(),
        ComponentKind::of::<ImmutableSeed>(),
    ]
}

/// Test host tag for `HostOwned`.
#[derive(Clone, Copy)]
struct Singleton;

fn build_app() -> App {
    let mut app = App::new();
    app.add_plugins(ServerPlugin::new(
        naia_bevy_server::ServerPluginConfig::new(
            ServerConfig::default(),
            protocol(),
            naia_bevy_server::Topology::SimIntegration(
                naia_bevy_server::SimIntegrationConfig::default(),
            ),
        ),
    ));
    app
}

fn build_app_with_custom_schedule() -> App {
    let mut app = App::new();
    app.add_schedule(Schedule::new(CustomSchedule));
    app.add_plugins(ServerPlugin::new(
        naia_bevy_server::ServerPluginConfig::new(
            ServerConfig::default(),
            protocol(),
            naia_bevy_server::Topology::SimIntegration(
                naia_bevy_server::SimIntegrationConfig::default().with_schedule(CustomSchedule),
            ),
        ),
    ));
    app
}

/// Spawns one entity carrying every registered component under `HostOwned`,
/// runs `Update` once, and drains the `HostSyncEvent::Insert` kinds in the
/// order the tracker systems wrote them.
fn insert_kind_sequence(app: &mut App) -> Vec<ComponentKind> {
    app.world_mut().spawn((
        HostOwned::new::<Singleton>(),
        Position::new(1.0, 2.0),
        Velocity::new_complete(3.0, 4.0),
        ImmutableLabel,
        ImmutableSeed::new(5, 6),
    ));
    app.update();

    let events = app
        .world()
        .get_resource::<Messages<HostSyncEvent>>()
        .expect("HostSyncEvent Messages resource present");
    events
        .iter_current_update_messages()
        .filter_map(|event| match event {
            HostSyncEvent::Insert(_, _, kind) => Some(*kind),
            _ => None,
        })
        .collect()
}

#[test]
fn two_apps_register_identical_tracker_order_in_registration_order() {
    let mut app_a = build_app();
    let mut app_b = build_app();

    let sequence_a = insert_kind_sequence(&mut app_a);
    let sequence_b = insert_kind_sequence(&mut app_b);

    assert_eq!(
        sequence_a, sequence_b,
        "two independently built apps must register the identical tracker order",
    );
    assert_eq!(
        sequence_a,
        expected_kind_order(),
        "tracker order must be protocol registration order",
    );
}

/// A pair is in scope when BOTH sides are naia change-detection systems:
/// the per-kind `on_component_added` / `on_component_removed` trackers or
/// the trailing `on_despawn`.
fn is_naia_tracker_system(full_name: &str) -> bool {
    full_name.contains("change_detection::on_component_added")
        || full_name.contains("change_detection::on_component_removed")
        || full_name.contains("change_detection::on_despawn")
}

#[test]
fn update_schedule_has_no_ambiguities_among_naia_trackers() {
    let mut app = build_app();
    // Name map BEFORE the first tick: the build moves system inners out of
    // the graph, so keys only resolve to names pre-run.
    let mut names_by_key: HashMap<String, String> = HashMap::new();
    {
        let schedule = app.get_schedule(Update).expect("Update schedule");
        for (key, system, _) in schedule.graph().systems.iter() {
            names_by_key.insert(format!("{key:?}"), system.name().to_string());
        }
    }

    app.world_mut().spawn((
        HostOwned::new::<Singleton>(),
        Position::new(7.0, 8.0),
        Velocity::new_complete(9.0, 10.0),
        ImmutableLabel,
        ImmutableSeed::new(11, 12),
    ));
    // The tick builds the schedule and records every conflicting pair.
    app.update();

    let schedule = app.get_schedule(Update).expect("Update schedule");
    let conflicting = &schedule.graph().conflicting_systems().0;
    let mut in_scope = Vec::new();
    for (a, b, _) in conflicting.iter() {
        let name_a = names_by_key
            .get(&format!("{a:?}"))
            .cloned()
            .unwrap_or_else(|| "?".to_string());
        let name_b = names_by_key
            .get(&format!("{b:?}"))
            .cloned()
            .unwrap_or_else(|| "?".to_string());
        if is_naia_tracker_system(&name_a) && is_naia_tracker_system(&name_b) {
            in_scope.push(format!("{name_a} AND {name_b}"));
        }
    }
    println!(
        "Update schedule: {} total conflicting pairs, {} among naia trackers",
        conflicting.len(),
        in_scope.len()
    );
    assert!(
        in_scope.is_empty(),
        "Update schedule has {} ambiguous pairs among naia trackers:\n{}",
        in_scope.len(),
        in_scope.join("\n")
    );
}

#[test]
fn custom_schedule_has_no_ambiguities_among_naia_trackers() {
    let mut app = build_app_with_custom_schedule();
    // Name map BEFORE the first run: the build moves system inners out of
    // the graph, so keys only resolve to names pre-run.
    let mut names_by_key: HashMap<String, String> = HashMap::new();
    {
        let schedules = app
            .world()
            .get_resource::<Schedules>()
            .expect("Schedules resource present");
        let schedule = schedules.get(CustomSchedule).expect("CustomSchedule");
        for (key, system, _) in schedule.graph().systems.iter() {
            names_by_key.insert(format!("{key:?}"), system.name().to_string());
        }
    }

    app.world_mut().spawn((
        HostOwned::new::<Singleton>(),
        Position::new(7.0, 8.0),
        Velocity::new_complete(9.0, 10.0),
        ImmutableLabel,
        ImmutableSeed::new(11, 12),
    ));
    // The run builds the schedule and records every conflicting pair.
    app.world_mut().run_schedule(CustomSchedule);

    let schedules = app
        .world()
        .get_resource::<Schedules>()
        .expect("Schedules resource present");
    let schedule = schedules.get(CustomSchedule).expect("CustomSchedule");
    let conflicting = &schedule.graph().conflicting_systems().0;
    let mut in_scope = Vec::new();
    for (a, b, _) in conflicting.iter() {
        let name_a = names_by_key
            .get(&format!("{a:?}"))
            .cloned()
            .unwrap_or_else(|| "?".to_string());
        let name_b = names_by_key
            .get(&format!("{b:?}"))
            .cloned()
            .unwrap_or_else(|| "?".to_string());
        if is_naia_tracker_system(&name_a) && is_naia_tracker_system(&name_b) {
            in_scope.push(format!("{name_a} AND {name_b}"));
        }
    }
    println!(
        "CustomSchedule: {} total conflicting pairs, {} among naia trackers",
        conflicting.len(),
        in_scope.len()
    );
    assert!(
        in_scope.is_empty(),
        "CustomSchedule has {} ambiguous pairs among naia trackers:\n{}",
        in_scope.len(),
        in_scope.join("\n")
    );
}
