//! MISSION_USER_ONLY_SEES_SIM Phase D.3b.2 (2026-05-19) — Coord-only /
//! handle-direct host-sync ops.
//!
//! `drain_host_sync_into_pipeline` previously reassembled a `WorldServer`
//! via `run_with_world_server` to call `is_listening` +
//! `entity_authority_status` + `insert/remove/despawn_component_worldless`.
//! D.3b.2 exposes those handle-direct:
//! - `SendHandle::is_listening` (mirrors `WorldServer::is_listening` —
//!   `send_io.is_loaded()`),
//! - `SimHandle::entity_authority_status` (pure shared read),
//! - `SendHandle::{insert,remove}_component_worldless` (shared gwm +
//!   per-connection scope state),
//! - `SendHandle::despawn_entity_worldless(&mut sim_handle.state, ..)` (adds
//!   Coord-side priority-mirror + room-cache cleanup).
//!
//! Each `*_worldless` method mirrors its `WorldServer` namesake
//! field-for-field, so the helper's wire output is byte-identical to the
//! prior reassembly path. The existing `WorldServer::*` methods are
//! unchanged and serve as the parity oracle here.
//!
//! Coverage:
//! - the new handle-direct drain produces byte-identical post-state
//!   observables (`entity_owner` + `entity_authority_status`) to a
//!   WorldServer-reassembly version of the same drain, across insert +
//!   remove + despawn,
//! - the queries (`is_listening`, `entity_authority_status`) agree with
//!   their `WorldServer` namesakes,
//! - `is_listening == false` short-circuits the drain (no events
//!   consumed-with-effect; entity untouched),
//! - a despawn fully clears the entity from shared state (owner reverts to
//!   `Local`, auth-status `None`).

use std::any::TypeId;
use std::time::Duration;

use bevy_app::App;
use bevy_ecs::{entity::Entity, message::Messages, world::World};

use naia_bevy_server::{
    drain_host_sync_into_pipeline,
    pipeline_actors::{run_with_world_server, spawn_server_handles, SimHandle},
    EntityOwner, Plugin as ServerPlugin, RecvHandle, SendHandle, ServerConfig,
};
use naia_bevy_shared::{HostSyncEvent, Protocol as BevyProtocol};
use naia_shared::{ComponentKind, EntityAuthStatus};
use naia_test_harness::test_protocol::Position;

fn protocol() -> naia_shared::Protocol {
    let mut p = BevyProtocol::builder();
    p.register_component::<Position>();
    p.tick_interval(Duration::from_micros(100));
    let bevy_proto = p.build();
    bevy_proto.into()
}

fn protocol_bevy() -> naia_bevy_shared::Protocol {
    let mut p = BevyProtocol::builder();
    p.register_component::<Position>();
    p.tick_interval(Duration::from_micros(100));
    p.build()
}

use std::sync::atomic::{AtomicU16, Ordering};
static NEXT_PORT: AtomicU16 = AtomicU16::new(31700);
fn next_addr() -> String {
    let port = NEXT_PORT.fetch_add(1, Ordering::Relaxed);
    format!("127.0.0.1:{port}")
}

/// Listening handles — the worldless ops only run when `is_listening()`
/// is true (the drain short-circuits otherwise, matching the legacy
/// `world_to_host_sync` guard). Loads the io exactly like
/// `send_scope_changes_drain.rs::handles_listening`.
fn handles() -> (SimHandle<Entity>, RecvHandle<Entity>, SendHandle<Entity>) {
    use naia_server::transport::local::{LocalServerSocket, LocalTransportHub, Socket};

    let (sim_handle, recv, send) =
        spawn_server_handles::<Entity, _>(ServerConfig::default(), protocol());

    let hub = LocalTransportHub::new(next_addr().parse().unwrap());
    let socket = Socket::new(LocalServerSocket::new(hub), None);
    let (_a, _b, ps, pr) = naia_server::transport::Socket::listen(Box::new(socket));

    let (sim_handle, recv, send, ()) = run_with_world_server(sim_handle, recv, send, |ws| {
        ws.io_load(ps, pr);
    });
    (sim_handle, recv, send)
}

/// Build a Sim-side bevy app holding the host-sync plumbing + a registered
/// entity carrying a `Position` component. Returns the app + entity.
fn sim_app_with_entity() -> (App, Entity) {
    let mut sim_app = App::new();
    sim_app.add_plugins(ServerPlugin::sim_integration(
        ServerConfig::default(),
        protocol_bevy(),
    ));
    // Ensure the HostSyncEvent message buffer exists (the drain reads it).
    sim_app
        .world_mut()
        .insert_resource(Messages::<HostSyncEvent>::default());
    let entity = sim_app.world_mut().spawn(Position::new(1.0, 2.0)).id();
    (sim_app, entity)
}

fn write_events(world: &mut World, events: Vec<HostSyncEvent>) {
    let mut buf = world.resource_mut::<Messages<HostSyncEvent>>();
    for e in events {
        buf.write(e);
    }
}

/// Snapshot of public shared-state observables for an entity.
#[derive(Debug, PartialEq)]
struct Observed {
    owner: EntityOwner,
    auth: Option<EntityAuthStatus>,
}

fn observe(sim_handle: &SimHandle<Entity>, entity: &Entity) -> Observed {
    Observed {
        owner: sim_handle.entity_owner(entity),
        auth: sim_handle.entity_authority_status(entity),
    }
}

#[test]
fn handle_direct_insert_matches_legacy() {
    // Insert a component via the new handle-direct drain vs a
    // WorldServer-reassembly version of the identical drain. Post-state
    // observables must agree byte-for-byte.

    fn run(handle_direct: bool) -> Observed {
        let (mut sim_app, entity) = sim_app_with_entity();
        let (mut sim_handle, recv, send) = handles();
        sim_handle.enable_entity_replication(&entity);

        let pos_kind = ComponentKind::of::<Position>();
        let events = vec![HostSyncEvent::Insert(TypeId::of::<()>(), entity, pos_kind)];

        let (sim_handle, _recv, _send) = if handle_direct {
            write_events(sim_app.world_mut(), events);
            drain_host_sync_into_pipeline(sim_app.world_mut(), sim_handle, recv, send)
        } else {
            // Legacy oracle: reassemble WorldServer and run the same body.
            use naia_bevy_shared::{WorldMutType, WorldProxyMut};
            use std::ops::DerefMut;
            let (sim_handle, recv, send, ()) =
                run_with_world_server(sim_handle, recv, send, |ws| {
                    if !ws.is_listening() {
                        return;
                    }
                    for event in events {
                        if let HostSyncEvent::Insert(_h, e, kind) = event {
                            if ws.entity_authority_status(&e) == Some(EntityAuthStatus::Denied) {
                                continue;
                            }
                            let mut proxy = WorldProxyMut::proxy_mut(sim_app.world_mut());
                            let Some(mut cm) = proxy.component_mut_of_kind(&e, &kind) else {
                                continue;
                            };
                            ws.insert_component_worldless(&e, DerefMut::deref_mut(&mut cm));
                        }
                    }
                });
            (sim_handle, recv, send)
        };

        observe(&sim_handle, &entity)
    }

    let from_handle = run(true);
    let from_legacy = run(false);
    assert_eq!(
        from_handle, from_legacy,
        "handle-direct insert_component_worldless must produce byte-identical \
         shared-state observables to the legacy WorldServer path",
    );
    // Sanity: the entity is still a server-owned, available entity.
    assert!(matches!(from_handle.owner, EntityOwner::Server));
}

#[test]
fn handle_direct_remove_matches_legacy() {
    // Insert then remove via each path; compare post-state.

    fn run(handle_direct: bool) -> Observed {
        let (mut sim_app, entity) = sim_app_with_entity();
        let (mut sim_handle, mut recv, mut send) = handles();
        sim_handle.enable_entity_replication(&entity);
        let pos_kind = ComponentKind::of::<Position>();

        // Insert first (handle-direct in both cases — that path is covered
        // by the insert test; here we isolate remove parity).
        {
            write_events(
                sim_app.world_mut(),
                vec![HostSyncEvent::Insert(TypeId::of::<()>(), entity, pos_kind)],
            );
            let (c, r, s) =
                drain_host_sync_into_pipeline(sim_app.world_mut(), sim_handle, recv, send);
            sim_handle = c;
            recv = r;
            send = s;
        }

        let events = vec![HostSyncEvent::Remove(TypeId::of::<()>(), entity, pos_kind)];
        let (sim_handle, _recv, _send) = if handle_direct {
            write_events(sim_app.world_mut(), events);
            drain_host_sync_into_pipeline(sim_app.world_mut(), sim_handle, recv, send)
        } else {
            let (sim_handle, recv, send, ()) =
                run_with_world_server(sim_handle, recv, send, |ws| {
                    if !ws.is_listening() {
                        return;
                    }
                    for event in events {
                        if let HostSyncEvent::Remove(_h, e, kind) = event {
                            if ws.entity_authority_status(&e) == Some(EntityAuthStatus::Denied) {
                                continue;
                            }
                            ws.remove_component_worldless(&e, &kind);
                        }
                    }
                });
            (sim_handle, recv, send)
        };

        observe(&sim_handle, &entity)
    }

    let from_handle = run(true);
    let from_legacy = run(false);
    assert_eq!(
        from_handle, from_legacy,
        "handle-direct remove_component_worldless must match legacy WorldServer path",
    );
    assert!(matches!(from_handle.owner, EntityOwner::Server));
}

#[test]
fn handle_direct_despawn_matches_legacy_and_clears_state() {
    // Despawn via each path; compare post-state. After despawn the entity
    // must be fully gone from shared state (owner -> Local, auth -> None).

    fn run(handle_direct: bool) -> Observed {
        let (mut sim_app, entity) = sim_app_with_entity();
        let (mut sim_handle, recv, send) = handles();
        sim_handle.enable_entity_replication(&entity);

        let events = vec![HostSyncEvent::Despawn(TypeId::of::<()>(), entity)];
        let (sim_handle, _recv, _send) = if handle_direct {
            write_events(sim_app.world_mut(), events);
            drain_host_sync_into_pipeline(sim_app.world_mut(), sim_handle, recv, send)
        } else {
            let (sim_handle, recv, send, ()) =
                run_with_world_server(sim_handle, recv, send, |ws| {
                    if !ws.is_listening() {
                        return;
                    }
                    for event in events {
                        if let HostSyncEvent::Despawn(_h, e) = event {
                            if ws.entity_authority_status(&e) == Some(EntityAuthStatus::Denied) {
                                continue;
                            }
                            ws.despawn_entity_worldless(&e);
                        }
                    }
                });
            (sim_handle, recv, send)
        };

        observe(&sim_handle, &entity)
    }

    let from_handle = run(true);
    let from_legacy = run(false);
    assert_eq!(
        from_handle, from_legacy,
        "handle-direct despawn_entity_worldless must match legacy WorldServer path",
    );
    // Despawn fully clears the entity from shared state.
    assert_eq!(from_handle.owner, EntityOwner::Local);
    assert_eq!(from_handle.auth, None);
}

#[test]
fn queries_match_world_server() {
    // is_listening + entity_authority_status read identically through the
    // handles vs the reassembled WorldServer.
    let (mut sim_handle, recv, send) = handles();
    let mut sim_app = App::new();
    sim_app.add_plugins(ServerPlugin::sim_integration(
        ServerConfig::default(),
        protocol_bevy(),
    ));
    let entity = sim_app.world_mut().spawn(()).id();
    sim_handle.enable_entity_replication(&entity);

    let handle_is_listening = send.is_listening();
    let handle_auth = sim_handle.entity_authority_status(&entity);

    let (_c, _r, _s, (ws_is_listening, ws_auth)) =
        run_with_world_server(sim_handle, recv, send, |ws| {
            (ws.is_listening(), ws.entity_authority_status(&entity))
        });

    assert_eq!(
        handle_is_listening, ws_is_listening,
        "SendHandle::is_listening must equal WorldServer::is_listening",
    );
    assert_eq!(
        handle_auth, ws_auth,
        "SimHandle::entity_authority_status must equal WorldServer::entity_authority_status",
    );
}

#[test]
fn not_listening_short_circuits() {
    // A non-listening server (spawn_server_handles without io_load) must
    // skip the drain entirely — a queued Despawn leaves the entity intact,
    // matching the legacy `world_to_host_sync` `is_listening` guard.
    let (mut sim_handle, recv, send) =
        spawn_server_handles::<Entity, _>(ServerConfig::default(), protocol());
    assert!(!send.is_listening(), "fresh handles are not listening");

    let mut sim_app = App::new();
    sim_app.add_plugins(ServerPlugin::sim_integration(
        ServerConfig::default(),
        protocol_bevy(),
    ));
    sim_app
        .world_mut()
        .insert_resource(Messages::<HostSyncEvent>::default());
    let entity = sim_app.world_mut().spawn(Position::new(1.0, 2.0)).id();
    sim_handle.enable_entity_replication(&entity);

    write_events(
        sim_app.world_mut(),
        vec![HostSyncEvent::Despawn(TypeId::of::<()>(), entity)],
    );
    let (sim_handle, _recv, _send) =
        drain_host_sync_into_pipeline(sim_app.world_mut(), sim_handle, recv, send);

    // Not listening -> drain skipped -> entity still server-owned.
    assert!(
        matches!(sim_handle.entity_owner(&entity), EntityOwner::Server),
        "not-listening drain must not despawn the entity",
    );
}

#[test]
fn empty_queue_is_noop() {
    // No HostSyncEvents -> handles returned unchanged, entity untouched.
    let (mut sim_app, entity) = sim_app_with_entity();
    let (mut sim_handle, recv, send) = handles();
    sim_handle.enable_entity_replication(&entity);
    let before = observe(&sim_handle, &entity);

    let (sim_handle, _recv, _send) =
        drain_host_sync_into_pipeline(sim_app.world_mut(), sim_handle, recv, send);

    let after = observe(&sim_handle, &entity);
    assert_eq!(
        before, after,
        "empty queue must leave shared state untouched"
    );
    assert!(matches!(after.owner, EntityOwner::Server));
}
