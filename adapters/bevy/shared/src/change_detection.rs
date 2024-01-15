use bevy_ecs::{
    entity::Entity,
    event::EventWriter,
    prelude::Event,
    query::{Added, With},
    removal_detection::RemovedComponents,
    system::Query,
};

use naia_shared::{ComponentKind, Replicate};

use crate::HostOwned;

#[derive(Event)]
pub enum HostSyncEvent {
    Insert(Entity, ComponentKind),
    Remove(Entity, ComponentKind),
    Despawn(Entity),
}

pub fn on_despawn<T: Send + Sync + 'static>(
    mut events: EventWriter<HostSyncEvent>,
    query: Query<Entity>,
    mut removals: RemovedComponents<HostOwned<T>>,
) {
    for entity in removals.read() {
        if let Ok(_) = query.get(entity) {
            // Entity is still alive, expected if Auth is reset on Delegated Entity
        } else {
            // info!("despawn on HostOwned entity: {:?}", entity);
            events.send(HostSyncEvent::Despawn(entity));
        }
    }
}

pub fn on_component_added<T: Send + Sync + 'static, R: Replicate>(
    mut events: EventWriter<HostSyncEvent>,
    query: Query<Entity, (Added<R>, With<HostOwned<T>>)>,
) {
    for entity in query.iter() {
        events.send(HostSyncEvent::Insert(entity, ComponentKind::of::<R>()));
    }
}

pub fn on_component_removed<T: Send + Sync + 'static, R: Replicate>(
    mut events: EventWriter<HostSyncEvent>,
    query: Query<Entity, With<HostOwned<T>>>,
    mut removals: RemovedComponents<R>,
) {
    for entity in removals.read() {
        if let Ok(_) = query.get(entity) {
            events.send(HostSyncEvent::Remove(entity, ComponentKind::of::<R>()));
        }
    }
}
