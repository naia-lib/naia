use std::{hash::Hash, marker::PhantomData};

use naia_shared::{ReplicaRefWrapper, ReplicateSafe, WorldRefType};

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

    pub fn has_component<R: ReplicateSafe>(&self) -> bool {
        self.world.has_component::<R>(&self.entity)
    }

    pub fn component<R: ReplicateSafe>(&self) -> Option<ReplicaRefWrapper<R>> {
        self.world.component::<R>(&self.entity)
    }
}

// // EntityMut
// pub struct EntityMut<P: Protocolize, E: Copy + Eq + Hash, W: WorldMutType<P, E>> {
//     world: W,
//     entity: E,
//     phantom_p: PhantomData<P>,
// }
//
// impl<'c, P: Protocolize, E: Copy + Eq + Hash, W: WorldMutType<P, E>> EntityMut<P, E, W> {
//     pub fn new(world: W, entity: &E) -> Self {
//         EntityMut {
//             world,
//             entity: *entity,
//             phantom_p: PhantomData,
//         }
//     }
// }
