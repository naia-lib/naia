use std::marker::PhantomData;

use naia_shared::{ComponentRef, EntityType, ProtocolType, ReplicateSafe, WorldRefType};

use super::client::Client;

// EntityRef
pub struct EntityRef<'s, P: ProtocolType, K: EntityType, W: WorldRefType<P, K>> {
    client: &'s Client<P, K>,
    world: W,
    id: K,
}

impl<'s, P: ProtocolType, K: EntityType, W: WorldRefType<P, K>> EntityRef<'s, P, K, W> {
    pub fn new(client: &'s Client<P, K>, world: W, key: &K) -> Self {
        EntityRef {
            client,
            world,
            id: *key,
        }
    }

    pub fn id(&self) -> K {
        self.id
    }

    pub fn has_component<R: ReplicateSafe<P>>(&self) -> bool {
        return self.world.has_component::<R>(&self.id);
    }

    pub fn component<R: ReplicateSafe<P>>(&self) -> Option<ComponentRef<P, R>> {
        return self.world.get_component::<R>(&self.id);
    }

    pub fn is_owned(&self) -> bool {
        return self.client.entity_is_owned(&self.id);
    }

    pub fn prediction(self) -> PredictedEntityRef<P, K, W> {
        if !self.is_owned() {
            panic!("Attempted to call .prediction() on an un-owned Entity!");
        }
        return PredictedEntityRef::new(self.world, &self.id);
    }
}

// PredictedEntityRef
pub struct PredictedEntityRef<P: ProtocolType, K: EntityType, W: WorldRefType<P, K>> {
    world: W,
    id: K,
    phantom: PhantomData<P>,
}

impl<P: ProtocolType, K: EntityType, W: WorldRefType<P, K>> PredictedEntityRef<P, K, W> {
    pub fn new(world: W, key: &K) -> Self {
        PredictedEntityRef {
            world,
            id: *key,
            phantom: PhantomData,
        }
    }

    pub fn id(&self) -> K {
        self.id
    }

    pub fn has_component<R: ReplicateSafe<P>>(&self) -> bool {
        return self.world.has_component::<R>(&self.id);
    }

    pub fn component<R: ReplicateSafe<P>>(&self) -> Option<ComponentRef<P, R>> {
        return self.world.get_component::<R>(&self.id);
    }
}
