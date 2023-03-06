use bevy_ecs::{
    entity::Entity,
    event::EventWriter,
    query::{Added, With},
    removal_detection::RemovedComponents,
    system::Query,
};

use naia_shared::{ComponentKind, Replicate};

use crate::HostOwned;

pub struct HostComponentEvent(pub bool, pub Entity, pub ComponentKind);

pub fn on_component_added<R: Replicate>(
    mut host_components: EventWriter<HostComponentEvent>,
    mut query: Query<Entity, (Added<R>, With<HostOwned>)>,
) {
    for entity in query.iter_mut() {
        host_components.send(HostComponentEvent(true, entity, ComponentKind::of::<R>()));
    }
}

pub fn on_component_removed<R: Replicate>(
    mut host_components: EventWriter<HostComponentEvent>,
    query: Query<Entity, With<HostOwned>>,
    mut removals: RemovedComponents<R>,
) {
    for removal_entity in removals.iter() {
        if let Ok(entity) = query.get(removal_entity) {
            host_components.send(HostComponentEvent(false, entity, ComponentKind::of::<R>()));
        }
    }
}
