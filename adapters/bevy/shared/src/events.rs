use std::{any::Any, collections::HashMap};

use bevy_ecs::{entity::Entity, event::Events, world::World};

use naia_shared::WorldEvents;

use crate::{ComponentKind, Replicate};

mod naia_events {
    pub use naia_shared::{
        DespawnEntityEvent, InsertComponentEvent, RemoveComponentEvent, SpawnEntityEvent,
        UpdateComponentEvent,
    };
}

// SpawnEntityEvent
pub struct SpawnEntityEvent(pub Entity);

// DespawnEntityEvent
pub struct DespawnEntityEvent(pub Entity);

// InsertComponentEvent
pub struct InsertComponentEvents {
    inner: HashMap<ComponentKind, Vec<Entity>>,
}

impl InsertComponentEvents {
    pub fn new(inner: HashMap<ComponentKind, Vec<Entity>>) -> Self {
        Self { inner }
    }
    pub fn read<C: Replicate>(&self) -> Vec<Entity> {
        let component_kind = ComponentKind::of::<C>();
        if let Some(components) = self.inner.get(&component_kind) {
            return components.clone();
        }

        return Vec::new();
    }
}

// RemoveComponentEvents
pub struct RemoveComponentEvents {
    inner: HashMap<ComponentKind, Vec<(Entity, Box<dyn Replicate>)>>,
}

impl RemoveComponentEvents {
    pub fn new(inner: HashMap<ComponentKind, Vec<(Entity, Box<dyn Replicate>)>>) -> Self {
        Self { inner }
    }

    pub fn read<C: Replicate>(&self) -> Vec<(Entity, C)> {
        let mut output = Vec::new();

        let component_kind = ComponentKind::of::<C>();
        if let Some(components) = self.inner.get(&component_kind) {
            for (entity, boxed_component) in components {
                let boxed_any = boxed_component.copy_to_box().to_boxed_any();
                let component: C = Box::<dyn Any + 'static>::downcast::<C>(boxed_any)
                    .ok()
                    .map(|boxed_c| *boxed_c)
                    .unwrap();
                output.push((*entity, component));
            }
        }

        output
    }
}

pub struct BevyWorldEvents;
impl BevyWorldEvents {
    pub fn write_events(world_events: &mut WorldEvents<Entity>, world: &mut World) {
        // Despawn Entity Event
        if world_events.has::<naia_events::DespawnEntityEvent>() {
            let mut despawn_entity_event_writer = world
                .get_resource_mut::<Events<DespawnEntityEvent>>()
                .unwrap();
            for entity in world_events.read::<naia_events::DespawnEntityEvent>() {
                despawn_entity_event_writer.send(DespawnEntityEvent(entity));
            }
        }

        // Insert Component Event
        if world_events.has::<naia_events::InsertComponentEvents>() {
            let inserts = world_events.take_inserts().unwrap();
            let mut insert_component_event_writer = world
                .get_resource_mut::<Events<InsertComponentEvents>>()
                .unwrap();
            insert_component_event_writer.send(InsertComponentEvents::new(inserts));
        }

        // Remove Component Event
        if world_events.has::<naia_events::RemoveComponentEvents>() {
            let removes = world_events.take_removes().unwrap();
            let mut remove_component_event_writer = world
                .get_resource_mut::<Events<RemoveComponentEvents>>()
                .unwrap();

            remove_component_event_writer.send(RemoveComponentEvents::new(removes));
        }
    }
}
