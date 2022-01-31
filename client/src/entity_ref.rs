use std::{hash::Hash, marker::PhantomData};

use naia_shared::{Protocolize, ReplicaRefWrapper, ReplicateSafe, WorldRefType};

// EntityRef
pub struct EntityRef<P: Protocolize, E: Copy + Eq + Hash, W: WorldRefType<P, E>> {
    world: W,
    entity: E,
    phantom_p: PhantomData<P>
}

impl<P: Protocolize, E: Copy + Eq + Hash, W: WorldRefType<P, E>> EntityRef<P, E, W> {
    pub fn new(world: W, entity: &E) -> Self {
        EntityRef {
            world,
            entity: *entity,
            phantom_p: PhantomData,
        }
    }

    pub fn get(&self) -> E {
        self.entity
    }

    pub fn has_component<R: ReplicateSafe<P>>(&self) -> bool {
        return self.world.has_component::<R>(&self.entity);
    }

    pub fn component<R: ReplicateSafe<P>>(&self) -> Option<ReplicaRefWrapper<P, R>> {
        return self.world.get_component::<R>(&self.entity);
    }
}
