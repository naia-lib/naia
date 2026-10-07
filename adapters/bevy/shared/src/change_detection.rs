use std::any::TypeId;

use bevy_ecs::{
    component::Component,
    entity::Entity,
    lifecycle::RemovedComponents,
    message::{Message, Messages},
    query::{Added, Changed},
    system::{Query, ResMut},
};

use naia_shared::{ComponentKind, Replicate};

use crate::{HostOwned, HostOwnedMap};

/// Host-authority change event for a `HostOwned` entity: a component was
/// inserted or removed, or the entity itself was despawned.
#[derive(Message)]
pub enum HostSyncEvent {
    /// A replicated component was inserted on a host-owned entity; carries
    /// the host tag's `TypeId`, the entity, and the component's kind.
    Insert(TypeId, Entity, ComponentKind),
    /// A replicated component was removed from a host-owned entity; carries
    /// the host tag's `TypeId`, the entity, and the component's kind.
    Remove(TypeId, Entity, ComponentKind),
    /// A host-owned entity was despawned; carries the host tag's `TypeId`
    /// and the entity.
    Despawn(TypeId, Entity),
}

impl HostSyncEvent {
    /// Returns the host tag `TypeId` carried by this event.
    pub fn host_id(&self) -> TypeId {
        match self {
            HostSyncEvent::Insert(type_id, _, _) => *type_id,
            HostSyncEvent::Remove(type_id, _, _) => *type_id,
            HostSyncEvent::Despawn(type_id, _) => *type_id,
        }
    }
}

/// Bevy system: mirrors every entity whose `HostOwned` marker was just
/// added or changed into the `HostOwnedMap` resource.
pub fn on_host_owned_added(
    query: Query<(Entity, &HostOwned), Changed<HostOwned>>,
    mut host_owned_map: ResMut<HostOwnedMap>,
) {
    for (entity, host_owned) in query.iter() {
        host_owned_map.insert(entity, *host_owned);
    }
}

/// Bevy system: when a `HostOwned` marker is removed, emits a
/// [`HostSyncEvent::Despawn`] if the owning entity no longer exists
/// (removal via despawn rather than an auth reset).
pub fn on_despawn(
    mut events: ResMut<Messages<HostSyncEvent>>,
    query: Query<Entity>,
    mut removals: RemovedComponents<HostOwned>,
    mut host_owned_map: ResMut<HostOwnedMap>,
) {
    for entity in removals.read() {
        if query.get(entity).is_ok() {
            // entity still alive — HostOwned was removed due to auth reset, not despawn
        } else if let Some(host_owned) = host_owned_map.remove(&entity) {
            events.write(HostSyncEvent::Despawn(host_owned.type_id(), entity));
        }
    }
}

/// Bevy system: emits a [`HostSyncEvent::Insert`] whenever component `R`
/// is newly added on a host-owned entity.
pub fn on_component_added<R: Replicate + Component>(
    mut events: ResMut<Messages<HostSyncEvent>>,
    query: Query<(Entity, &HostOwned), Added<R>>,
) {
    for (entity, host_owned) in query.iter() {
        events.write(HostSyncEvent::Insert(
            host_owned.type_id(),
            entity,
            ComponentKind::of::<R>(),
        ));
    }
}

/// Bevy system: emits a [`HostSyncEvent::Remove`] whenever component `R`
/// is removed from a host-owned entity.
pub fn on_component_removed<R: Replicate + Component>(
    mut events: ResMut<Messages<HostSyncEvent>>,
    query: Query<&HostOwned>,
    mut removals: RemovedComponents<R>,
) {
    for entity in removals.read() {
        if let Ok(host_owned) = query.get(entity) {
            events.write(HostSyncEvent::Remove(
                host_owned.type_id(),
                entity,
                ComponentKind::of::<R>(),
            ));
        }
    }
}
