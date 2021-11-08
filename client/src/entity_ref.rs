use std::{hash::Hash, marker::PhantomData};

use naia_shared::{ProtocolType, ReplicaRefWrapper, ReplicateSafe, WorldRefType};

use super::client::Client;

// EntityRef
pub struct EntityRef<'s, P: ProtocolType, E: Copy + Eq + Hash, W: WorldRefType<P, E>> {
    client: &'s Client<P, E>,
    world: W,
    id: E,
}

impl<'s, P: ProtocolType, E: Copy + Eq + Hash, W: WorldRefType<P, E>> EntityRef<'s, P, E, W> {
    pub fn new(client: &'s Client<P, E>, world: W, key: &E) -> Self {
        EntityRef {
            client,
            world,
            id: *key,
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

    pub fn is_owned(&self) -> bool {
        return self.client.entity_is_owned(&self.id);
    }

    pub fn prediction(self) -> PredictedEntityRef<P, E, W> {
        if !self.is_owned() {
            panic!("Attempted to call .prediction() on an un-owned Entity!");
        }
        return PredictedEntityRef::new(self.world, &self.id);
    }
}

// PredictedEntityRef
pub struct PredictedEntityRef<P: ProtocolType, E: Copy, W: WorldRefType<P, E>> {
    world: W,
    id: E,
    phantom: PhantomData<P>,
}

impl<P: ProtocolType, E: Copy, W: WorldRefType<P, E>> PredictedEntityRef<P, E, W> {
    pub fn new(world: W, key: &E) -> Self {
        PredictedEntityRef {
            world,
            id: *key,
            phantom: PhantomData,
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
