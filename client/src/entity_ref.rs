use std::hash::Hash;

use crate::Client;
use naia_shared::{ReplicaMutWrapper, ReplicaRefWrapper, Replicate, WorldMutType, WorldRefType};

// EntityRef
pub struct EntityRef<E: Copy + Eq + Hash, W: WorldRefType<E>> {
    world: W,
    entity: E,
}

impl<E: Copy + Eq + Hash, W: WorldRefType<E>> EntityRef<E, W> {
    pub fn new(world: W, entity: &E) -> Self {
        EntityRef {
            world,
            entity: *entity,
        }
    }

    pub fn id(&self) -> E {
        self.entity
    }

    pub fn has_component<R: Replicate>(&self) -> bool {
        self.world.has_component::<R>(&self.entity)
    }

    pub fn component<R: Replicate>(&self) -> Option<ReplicaRefWrapper<R>> {
        self.world.component::<R>(&self.entity)
    }
}

// EntityMut
pub struct EntityMut<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>> {
    client: &'s mut Client<E>,
    world: W,
    entity: E,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>> EntityMut<'s, E, W> {
    pub(crate) fn new(server: &'s mut Client<E>, world: W, entity: &E) -> Self {
        EntityMut {
            client: server,
            world,
            entity: *entity,
        }
    }

    pub fn id(&self) -> E {
        self.entity
    }

    pub fn despawn(&mut self) {
        self.client.despawn_entity(&mut self.world, &self.entity);
    }

    // Components

    pub fn has_component<R: Replicate>(&self) -> bool {
        self.world.has_component::<R>(&self.entity)
    }

    pub fn component<R: Replicate>(&mut self) -> Option<ReplicaMutWrapper<R>> {
        self.world.component_mut::<R>(&self.entity)
    }

    pub fn insert_component<R: Replicate>(&mut self, component_ref: R) -> &mut Self {
        self.client
            .insert_component(&mut self.world, &self.entity, component_ref);

        self
    }

    pub fn insert_components<R: Replicate>(&mut self, mut component_refs: Vec<R>) -> &mut Self {
        while let Some(component_ref) = component_refs.pop() {
            self.insert_component(component_ref);
        }

        self
    }

    pub fn remove_component<R: Replicate>(&mut self) -> Option<R> {
        self.client
            .remove_component::<R, W>(&mut self.world, &self.entity)
    }
}
