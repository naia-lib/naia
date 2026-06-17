//! MISSION_USER_ONLY_SEES_SIM Phase D.2.2 (2026-05-19) —
//! `SimHandle::configure_entity_replication` deferred Send/World drain.
//!
//! `WorldServer::configure_entity_replication` (`world_server.rs:1423`)
//! fuses Coord (gwm) + Send (per-connection) + World (component-hook)
//! work in one synchronous body. The Coord-only `SimHandle` variant
//! performs the gwm writes immediately and defers the Send-side leaf
//! ops to `apply_pending_send_preamble` (via a
//! `ScopeChange::ConfigureReplication` payload) and the World-side hooks
//! to `apply_pending_world_hooks<W>` (called by the Sim system, which
//! holds the `&mut World`).
//!
//! Byte-identity strategy (same as D.2.1): the Send-side leaf ops are
//! verbatim relocations of the legacy `send_*` calls — byte-identical by
//! construction. What CAN diverge is the gwm decision tree and the
//! resulting replication state, so these tests pin the observable
//! gwm state (`entity_replication_config`, `entity_owner`,
//! `entity_authority_status`, `entity_is_delegated`) byte-for-byte
//! against the legacy fused path for each transition cyberlith uses
//! (the `drain_sim_tile_registrations` shape: server-owned entity,
//! `public().persist_on_scope_exit()` and
//! `delegated().persist_on_scope_exit()`).
//!
//! Additional coverage:
//! - World-hook deferral: the per-component diff mutators are NOT
//!   installed until `apply_pending_world_hooks` runs (the
//!   `pending_world_hooks` queue is non-empty after configure, empty
//!   after the drain).
//! - Same-tick composition (enable + configure) — gwm writes are
//!   immediate so a subsequent read in the same tick sees them.

use std::time::Duration;

use bevy_app::App;
use bevy_ecs::{entity::Entity, world::World};

use naia_bevy_server::{
    pipeline_actors::{run_with_world_server, spawn_server_handles, SimHandle},
    EntityAuthStatus, EntityOwner, Plugin as ServerPlugin, RecvHandle, ReplicationConfig,
    ScopeExit, SendHandle, ServerConfig,
};
use naia_bevy_shared::{Protocol as BevyProtocol, WorldProxyMut};
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

fn handles() -> (SimHandle<Entity>, RecvHandle<Entity>, SendHandle<Entity>) {
    spawn_server_handles::<Entity, _>(ServerConfig::default(), protocol())
}

fn sim_app() -> App {
    let mut app = App::new();
    app.add_plugins(ServerPlugin::sim_integration(
        ServerConfig::default(),
        protocol_bevy(),
    ));
    app
}

/// Drain the deferred World-side hooks onto the Sim app's world via a
/// `WorldMut` proxy (the `WorldMutType<Entity>` impl the hooks require).
fn drain_world_hooks(sim_handle: &SimHandle<Entity>, app: &mut App) {
    let world: &mut World = app.world_mut();
    let mut proxy = WorldProxyMut::proxy_mut(world);
    sim_handle.apply_pending_world_hooks(&mut proxy);
}

/// Observable gwm state read back via reassembly — the byte-identity
/// surface.
type Observables = (
    ReplicationConfig,
    EntityOwner,
    Option<EntityAuthStatus>,
    bool,
);

fn read_observables(
    sim_handle: SimHandle<Entity>,
    recv: RecvHandle<Entity>,
    send: SendHandle<Entity>,
    entity: &Entity,
) -> (
    SimHandle<Entity>,
    RecvHandle<Entity>,
    SendHandle<Entity>,
    Observables,
) {
    // Owner via the public `SimHandle::entity_owner` (the `WorldServer`
    // namesake is `pub(crate)`).
    let owner = sim_handle.entity_owner(entity);
    let (sim_handle, recv, send, (cfg, auth, delegated)) =
        run_with_world_server(sim_handle, recv, send, |ws| {
            let cfg = ws
                .entity_replication_config(entity)
                .expect("entity registered");
            let auth = ws.entity_authority_status(entity);
            let delegated = ws.entity_is_delegated(entity);
            (cfg, auth, delegated)
        });
    (sim_handle, recv, send, (cfg, owner, auth, delegated))
}

/// Run the named transition via either the Coord-only deferred path or
/// the legacy fused path, then read back the observable gwm state.
fn run_transition(sim_path: bool, target: ReplicationConfig) -> Observables {
    let (mut sim_handle, recv, send) = handles();
    let mut app = sim_app();
    let entity = app.world_mut().spawn(()).id();

    // Register server-owned (cyberlith tile shape).
    sim_handle.enable_entity_replication(&entity);

    let (sim_handle, recv, send) = if sim_path {
        let mut send = send;
        sim_handle.configure_entity_replication(&entity, target);
        // Deferred drains: Send-side via preamble, World-side via the
        // world-hook drain on the Sim world.
        send.apply_pending_send_preamble();
        drain_world_hooks(&sim_handle, &mut app);
        (sim_handle, recv, send)
    } else {
        let world: &mut World = app.world_mut();
        let mut proxy = WorldProxyMut::proxy_mut(world);
        let (sim_handle, recv, send, ()) = run_with_world_server(sim_handle, recv, send, |ws| {
            ws.configure_entity_replication(&mut proxy, &entity, target);
        });
        (sim_handle, recv, send)
    };

    let (_sim_handle, _recv, _send, obs) = read_observables(sim_handle, recv, send, &entity);
    obs
}

#[test]
fn public_scope_exit_toggle_matches_legacy() {
    // cyberlith tile path: server-owned, Public → Public+Persist
    // (publicity unchanged, scope_exit Despawn → Persist). Pure
    // scope_exit branch (no publish/unpublish/delegation).
    let target = ReplicationConfig::public().persist_on_scope_exit();
    let from_sim = run_transition(true, target);
    let from_legacy = run_transition(false, target);
    assert_eq!(
        from_sim, from_legacy,
        "Public scope_exit toggle must be byte-identical (Coord vs legacy)",
    );
    assert_eq!(from_sim.0, target);
    assert_eq!(from_sim.0.scope_exit, ScopeExit::Persist);
    assert!(matches!(from_sim.1, EntityOwner::Server));
}

#[test]
fn server_owned_delegated_matches_legacy() {
    // cyberlith tile path: server-owned, Public → Delegated+Persist.
    // Hits `entity_enable_delegation` with `client_origin = None`
    // (server-origin delegation). gwm transitions publicity to
    // Delegated and registers the entity with the auth handler.
    let target = ReplicationConfig::delegated().persist_on_scope_exit();
    let from_sim = run_transition(true, target);
    let from_legacy = run_transition(false, target);
    assert_eq!(
        from_sim, from_legacy,
        "server-owned Public → Delegated must be byte-identical (Coord vs legacy)",
    );
    assert_eq!(from_sim.0, target);
    assert!(from_sim.3, "entity must be delegated after configure");
    // Server-owned entity remains server-owned through delegation
    // (no client_origin migration).
    assert!(matches!(from_sim.1, EntityOwner::Server));
}

#[test]
fn delegated_back_to_public_matches_legacy() {
    // Delegated+Persist → Public+Persist (disable delegation, keep
    // scope_exit). Exercises `entity_disable_delegation` (server-owned).
    fn run(sim_path: bool) -> Observables {
        let (mut sim_handle, recv, send) = handles();
        let mut send = send;
        let mut app = sim_app();
        let entity = app.world_mut().spawn(()).id();
        sim_handle.enable_entity_replication(&entity);

        // First go to Delegated+Persist.
        let delegated = ReplicationConfig::delegated().persist_on_scope_exit();
        let public = ReplicationConfig::public().persist_on_scope_exit();

        let (sim_handle, recv, send) = if sim_path {
            sim_handle.configure_entity_replication(&entity, delegated);
            send.apply_pending_send_preamble();
            drain_world_hooks(&sim_handle, &mut app);
            sim_handle.configure_entity_replication(&entity, public);
            send.apply_pending_send_preamble();
            drain_world_hooks(&sim_handle, &mut app);
            (sim_handle, recv, send)
        } else {
            let (sim_handle, recv, send) = {
                let world: &mut World = app.world_mut();
                let mut proxy = WorldProxyMut::proxy_mut(world);
                let (c, r, s, ()) = run_with_world_server(sim_handle, recv, send, |ws| {
                    ws.configure_entity_replication(&mut proxy, &entity, delegated);
                });
                (c, r, s)
            };
            let world: &mut World = app.world_mut();
            let mut proxy = WorldProxyMut::proxy_mut(world);
            let (c, r, s, ()) = run_with_world_server(sim_handle, recv, send, |ws| {
                ws.configure_entity_replication(&mut proxy, &entity, public);
            });
            (c, r, s)
        };

        let (_c, _r, _s, obs) = read_observables(sim_handle, recv, send, &entity);
        obs
    }

    let from_sim = run(true);
    let from_legacy = run(false);
    assert_eq!(
        from_sim, from_legacy,
        "Delegated → Public round-trip must be byte-identical (Coord vs legacy)",
    );
    assert!(!from_sim.3, "entity must NOT be delegated after revert");
    assert_eq!(from_sim.0.scope_exit, ScopeExit::Persist);
}

#[test]
fn world_hooks_are_deferred_until_drain() {
    // The World-side component-hook registration must NOT happen during
    // `configure_entity_replication` — it is deferred to
    // `apply_pending_world_hooks`. Observable proxy: the
    // `pending_world_hooks` queue is non-empty after configure and empty
    // after the drain.
    let (mut sim_handle, _recv, mut send) = handles();
    let mut app = sim_app();
    let entity = app.world_mut().spawn(()).id();
    sim_handle.enable_entity_replication(&entity);

    // Public → Delegated installs a delegation world hook.
    let target = ReplicationConfig::delegated().persist_on_scope_exit();
    sim_handle.configure_entity_replication(&entity, target);

    assert!(
        sim_handle.pending_world_hooks_len() > 0,
        "world hooks must be queued (deferred), not applied during configure",
    );

    // Send-side preamble drain does NOT touch the world-hook queue.
    send.apply_pending_send_preamble();
    assert!(
        sim_handle.pending_world_hooks_len() > 0,
        "preamble drain must not consume world hooks",
    );

    // The world-hook drain clears the queue.
    drain_world_hooks(&sim_handle, &mut app);
    assert_eq!(
        sim_handle.pending_world_hooks_len(),
        0,
        "apply_pending_world_hooks must drain the world-hook queue",
    );
}

#[test]
fn no_op_configure_pushes_nothing() {
    // Configuring to the identical config is a no-op (matches the legacy
    // `prev_config == config` early return) — no Send ops, no world hooks.
    let (mut sim_handle, _recv, _send) = handles();
    let mut app = sim_app();
    let entity = app.world_mut().spawn(()).id();
    sim_handle.enable_entity_replication(&entity);

    // Server-spawned default is `public()`. Re-configuring to `public()`
    // is a no-op.
    sim_handle.configure_entity_replication(&entity, ReplicationConfig::public());
    assert_eq!(
        sim_handle.scope_change_queue_len(),
        0,
        "no-op configure must not push a ConfigureReplication scope change",
    );
    assert_eq!(
        sim_handle.pending_world_hooks_len(),
        0,
        "no-op configure must not push world hooks",
    );
}

#[test]
fn same_tick_enable_then_configure_composes() {
    // gwm writes are immediate, so a configure that reads the
    // just-enabled entity in the same tick sees the registration.
    // Mirrors cyberlith `drain_sim_tile_registrations` (enable +
    // configure back-to-back per tile).
    let (mut sim_handle, recv, mut send) = handles();
    let mut app = sim_app();
    let entity = app.world_mut().spawn(()).id();

    sim_handle.enable_entity_replication(&entity);
    let target = ReplicationConfig::public().persist_on_scope_exit();
    sim_handle.configure_entity_replication(&entity, target);
    send.apply_pending_send_preamble();
    drain_world_hooks(&sim_handle, &mut app);

    let (_c, _r, _s, obs) = read_observables(sim_handle, recv, send, &entity);
    assert_eq!(obs.0, target);
    assert_eq!(obs.0.scope_exit, ScopeExit::Persist);
}
