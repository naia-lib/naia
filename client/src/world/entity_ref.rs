use std::hash::Hash;

use naia_shared::{EntityAuthStatus, ReplicaRefWrapper, ReplicatedComponent, WorldRefType};

use crate::{Client, ReplicationConfig};

// EntityRef
pub struct EntityRef<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>> {
    client: &'s Client<E>,
    world: W,
    entity: E,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>> EntityRef<'s, E, W> {
    pub fn new(client: &'s Client<E>, world: W, entity: &E) -> Self {
        Self {
            client,
            world,
            entity: *entity,
        }
    }

    pub fn id(&self) -> E {
        self.entity
    }

    pub fn has_component<R: ReplicatedComponent>(&self) -> bool {
        self.world.has_component::<R>(&self.entity)
    }

    pub fn component<R: ReplicatedComponent>(&'_ self) -> Option<ReplicaRefWrapper<'_, R>> {
        self.world.component::<R>(&self.entity)
    }

    pub fn replication_config(&self) -> Option<ReplicationConfig> {
        self.client.entity_replication_config(&self.entity)
    }

    pub fn authority(&self) -> Option<EntityAuthStatus> {
        self.client.entity_authority_status(&self.entity)
    }
}

cfg_if! {
    if #[cfg(feature = "interior_visibility")] {
        
        use naia_shared::LocalEntity;
        
        impl<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>> EntityRef<'s, E, W> {
            
            pub fn local_entity(&self) -> LocalEntity {
                self.client.world_to_local_entity(&self.entity)
            }
        }
    }
}