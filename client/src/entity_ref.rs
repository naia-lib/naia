use std::{hash::Hash, marker::PhantomData};

use naia_shared::{Protocolize, ReplicaRefWrapper, ReplicateSafe, WorldRefType};

// EntityRef
pub struct EntityRef<P: Protocolize, E: Copy + Eq + Hash, W: WorldRefType<P, E>> {
    world: W,
    id: E,
    phantom_p: PhantomData<P>
}

impl<P: Protocolize, E: Copy + Eq + Hash, W: WorldRefType<P, E>> EntityRef<P, E, W> {
    pub fn new(world: W, key: &E) -> Self {
        EntityRef {
            world,
            id: *key,
            phantom_p: PhantomData,
        }
    }

    pub fn id(&self) -> E {
        self.id
    }

    pub fn has_component<R: ReplicateSafe<P>>(&self) -> bool {
        return self.world.has_component::<R>(&self.id);
    }

    pub fn component<R: ReplicateSafe<P>>(&self) -> Option<ReplicaRefWrapper<P, R>> {
        return self.world.get_component::<R>(&self.id);
    }
}
