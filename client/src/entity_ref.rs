use std::{hash::Hash, marker::PhantomData};

use naia_shared::{Protocolize, ReplicaRefWrapper, ReplicateSafe, WorldRefType};

use super::client::Client;

// EntityRef
pub struct EntityRef<P: Protocolize, E: Copy + Eq + Hash, W: WorldRefType<P, E>> {
    world: W,
    entity: E,
    phantom_p: PhantomData<P>,
}

impl<P: Protocolize, E: Copy + Eq + Hash, W: WorldRefType<P, E>> EntityRef<P, E, W> {
    pub fn new(world: W, entity: &E) -> Self {
        EntityRef {
            world,
            entity: *entity,
            phantom_p: PhantomData,
        }
    }

    pub fn id(&self) -> E {
        self.entity
    }

    pub fn has_component<R: ReplicateSafe<P>>(&self) -> bool {
        return self.world.has_component::<R>(&self.entity);
    }

    pub fn component<R: ReplicateSafe<P>>(&self) -> Option<ReplicaRefWrapper<P, R>> {
        return self.world.component::<R>(&self.entity);
    }
}

// EntityMut
pub struct EntityMut<'c, P: Protocolize, E: Copy + Eq + Hash> {
    client: &'c mut Client<P, E>,
    entity: E,
}

impl<'c, P: Protocolize, E: Copy + Eq + Hash> EntityMut<'c, P, E> {
    pub fn new(client: &'c mut Client<P, E>, entity: &E) -> Self {
        EntityMut {
            client,
            entity: *entity,
        }
    }

    // Messages

    pub fn send_message<R: ReplicateSafe<P>>(&mut self, message: &R) -> &mut Self {
        self.client.send_entity_message(&self.entity, message);

        self
    }
}
