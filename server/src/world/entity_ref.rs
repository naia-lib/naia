use std::hash::Hash;

use naia_shared::{ReplicaRefWrapper, Replicate, WorldRefType};

use crate::{ReplicationConfig, Server};

// EntityRef
pub struct EntityRef<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>> {
    server: &'s Server<E>,
    world: W,
    entity: E,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>> EntityRef<'s, E, W> {
    pub fn new(server: &'s Server<E>, world: W, entity: &E) -> Self {
        EntityRef {
            server,
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

    pub fn replication_config(&self) -> Option<ReplicationConfig> {
        self.server.entity_replication_config(&self.entity)
    }

    pub fn has_authority(&self) -> bool {
        todo!();
        //self.server.entity_has_authority(&self.entity)
    }
}
