//! The `Replication` marker component's observers (naia-lib/naia#182).
//!
//! Inserting `Replication` on an entity begins replication; removing it ends
//! replication. The `enable_replication` / `disable_replication` commands
//! converge onto the same marker, so both doors lead to the same room -- and
//! the guards below are what keep the two doors from double-registering an
//! entity when both are used.

use bevy_ecs::{
    lifecycle::{Add, Despawn, Remove},
    observer::On,
    system::Commands,
};

use naia_bevy_shared::HostOwned;

use super::{components::Replication, plugin::Singleton, server::Server};

/// Inserting the marker begins replication -- unless the entity is already
/// registered (the command path registers synchronously before a queued
/// marker insert flushes). Enabling twice trips the fail-loud double-enable
/// guard, so this check is load-bearing, not advisory.
///
/// Mirrors `enable_replication`'s `HostOwned` insert: the host-sync trackers
/// only follow entities carrying `HostOwned`, so without it the marker would
/// register the entity with naia yet never propagate anything.
pub fn on_replication_added(
    trigger: On<Add, Replication>,
    mut commands: Commands,
    mut server: Server,
) {
    let entity = trigger.event().entity;
    if server.replication_config(&entity).is_none() {
        server.enable_replication(&entity);
    }
    commands
        .entity(entity)
        .insert(HostOwned::new::<Singleton>());
}

/// Removing the marker ends replication. The guard keeps a
/// remove-after-command-disable a silent no-op instead of a redundant call.
/// Mirrors `disable_replication`'s `HostOwned` removal.
pub fn on_replication_removed(
    trigger: On<Remove, Replication>,
    mut commands: Commands,
    mut server: Server,
) {
    let entity = trigger.event().entity;
    if server.replication_config(&entity).is_some() {
        server.disable_replication(&entity);
    }
    commands.entity(entity).remove::<HostOwned>();
}

/// Despawn ends replication exactly like marker removal: despawn fires
/// `Despawn` rather than `Remove`, so it needs its own observer. If both
/// fire for one despawn, the guard makes the second a silent no-op.
/// (No `HostOwned` removal: despawn strips all components, and the
/// `RemovedComponents<HostOwned>` tracker already emits the despawn event.)
pub fn on_replication_despawned(trigger: On<Despawn, Replication>, mut server: Server) {
    let entity = trigger.event().entity;
    if server.replication_config(&entity).is_some() {
        server.disable_replication(&entity);
    }
}
