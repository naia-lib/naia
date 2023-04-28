use std::hash::Hash;

use naia_shared::{ReplicaRefWrapper, Replicate, WorldRefType};

use crate::{Client, ReplicationConfig};

// EntityRef
pub struct EntityRef<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>> {
    client: &'s Client<E>,
    world: W,
    entity: E,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>> EntityRef<'s, E, W> {
    pub fn new(client: &'s Client<E>, world: W, entity: &E) -> Self {
        EntityRef {
            client,
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

    pub fn replication_config(&self) -> ReplicationConfig {
        self.client.entity_replication_config(&self.entity)
    }

    pub fn has_authority(&self) -> bool {
        self.client.entity_has_authority(&self.entity)
    }
}
