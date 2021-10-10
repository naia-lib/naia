use naia_shared::{EntityType, ProtocolType, Ref, Replicate, WorldRefType};

use super::client::Client;

// EntityRef
#[derive(Debug)]
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

    pub fn has_component<R: Replicate<P>>(&self) -> bool {
        return self.world.has_component::<R>(&self.id);
    }

    pub fn component<R: Replicate<P>>(&self) -> Option<Ref<R>> {
        return self.world.get_component::<R>(&self.id);
    }

    pub fn is_owned(&self) -> bool {
        return self.client.entity_is_owned(&self.id);
    }

    pub fn prediction(self) -> PredictedEntityRef<'s, P, K, W> {
        if !self.is_owned() {
            panic!("Attempted to call .prediction() on an un-owned Entity!");
        }
        return PredictedEntityRef::new(self.client, self.world, &self.id);
    }
}

// PredictedEntityRef
#[derive(Debug)]
pub struct PredictedEntityRef<'s, P: ProtocolType, K: EntityType, W: WorldRefType<P, K>> {
    client: &'s Client<P, K>,
    world: W,
    id: K,
}

impl<'s, P: ProtocolType, K: EntityType, W: WorldRefType<P, K>> PredictedEntityRef<'s, P, K, W> {
    pub fn new(client: &'s Client<P, K>, world: W, key: &K) -> Self {
        PredictedEntityRef {
            client,
            world,
            id: *key,
        }
    }

    pub fn id(&self) -> K {
        self.id
    }

    pub fn has_component<R: Replicate<P>>(&self) -> bool {
        return self.world.has_component::<R>(&self.id);
    }

    pub fn component<R: Replicate<P>>(&self) -> Option<Ref<R>> {
        return self.world.get_component::<R>(&self.id);
    }
}
