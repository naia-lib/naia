use bevy_ecs::{
    entity::Entity,
    event::EventWriter,
    query::{Added, With},
    removal_detection::RemovedComponents,
    system::Query,
};

use naia_shared::{ComponentKind, Replicate};

use crate::HostOwned;

pub enum HostSyncEvent {
    Insert(Entity, ComponentKind),
    Remove(Entity, ComponentKind),
    Despawn(Entity),
}

pub fn on_despawn(
    mut events: EventWriter<HostSyncEvent>,
    mut removals: RemovedComponents<HostOwned>,
) {
    for entity in removals.iter() {
        events.send(HostSyncEvent::Despawn(entity));
    }
}

pub fn on_component_added<R: Replicate>(
    mut events: EventWriter<HostSyncEvent>,
    query: Query<Entity, (Added<R>, With<HostOwned>)>,
) {
    for entity in query.iter() {
        events.send(HostSyncEvent::Insert(entity, ComponentKind::of::<R>()));
    }
}

pub fn on_component_removed<R: Replicate>(
    mut events: EventWriter<HostSyncEvent>,
    query: Query<Entity, With<HostOwned>>,
    mut removals: RemovedComponents<R>,
) {
    for removal_entity in removals.iter() {
        if let Ok(entity) = query.get(removal_entity) {
            events.send(HostSyncEvent::Remove(entity, ComponentKind::of::<R>()));
        }
    }
}
