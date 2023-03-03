use std::{any::Any, collections::HashMap};

use bevy_ecs::entity::Entity;

use crate::{ComponentKind, Replicate};

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
