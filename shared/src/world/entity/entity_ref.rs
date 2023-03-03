use std::hash::Hash;

use crate::{ReplicaRefWrapper, Replicate, WorldRefType};

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
