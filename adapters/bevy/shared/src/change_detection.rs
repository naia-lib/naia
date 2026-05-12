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

#[derive(Message)]
pub enum HostSyncEvent {
    Insert(TypeId, Entity, ComponentKind),
    Remove(TypeId, Entity, ComponentKind),
    Despawn(TypeId, Entity),
}

impl HostSyncEvent {
    pub fn host_id(&self) -> TypeId {
        match self {
            HostSyncEvent::Insert(type_id, _, _) => *type_id,
            HostSyncEvent::Remove(type_id, _, _) => *type_id,
            HostSyncEvent::Despawn(type_id, _) => *type_id,
        }
    }
}

pub fn on_host_owned_added(
    query: Query<(Entity, &HostOwned), Changed<HostOwned>>,
    mut host_owned_map: ResMut<HostOwnedMap>,
) {
    for (entity, host_owned) in query.iter() {
        host_owned_map.insert(entity, *host_owned);
    }
}

pub fn on_despawn(
    mut events: ResMut<Messages<HostSyncEvent>>,
    query: Query<Entity>,
    mut removals: RemovedComponents<HostOwned>,
    mut host_owned_map: ResMut<HostOwnedMap>,
) {
    for entity in removals.read() {
        if query.get(entity).is_ok() {
            // Entity is still alive, expected if Auth is reset on Delegated Entity
        } else {
            // The entity is gone. If we don't have a HostOwnedMap entry
            // for it, the entity was either:
            //   - never tracked (replicated by the server, host-owned by
            //     another client; despawn is observed locally but we
            //     never owned it),
            //   - already torn down via a prior pathway (Auth transitioned
            //     before the despawn arrived, the map entry was cleared
            //     out-of-band), or
            //   - despawned in the same frame it was spawned (compressed
            //     test timelines).
            // None of these are bugs — they just don't need a HostSyncEvent
            // emission. The previous `panic!` here turned harmless edge
            // cases into hard crashes, especially in test harnesses that
            // condense multiple game-loop frames into a single tick.
            if let Some(host_owned) = host_owned_map.remove(&entity) {
                events.write(HostSyncEvent::Despawn(host_owned.type_id(), entity));
            }
        }
    }
}

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
