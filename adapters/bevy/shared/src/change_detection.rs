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
            // entity still alive — HostOwned was removed due to auth reset, not despawn
        } else if let Some(host_owned) = host_owned_map.remove(&entity) {
            events.write(HostSyncEvent::Despawn(host_owned.type_id(), entity));
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
