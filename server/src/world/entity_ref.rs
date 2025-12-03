use std::hash::Hash;

use naia_shared::{EntityAuthStatus, ReplicaRefWrapper, ReplicatedComponent, WorldRefType};

use crate::{server::WorldServer, ReplicationConfig};

// EntityRef
pub struct EntityRef<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>> {
    server: &'s WorldServer<E>,
    world: W,
    entity: E,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>> EntityRef<'s, E, W> {
    pub(crate) fn new(server: &'s WorldServer<E>, world: W, entity: &E) -> Self {
        Self {
            server,
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
        self.server.entity_replication_config(&self.entity)
    }

    pub fn authority(&self) -> Option<EntityAuthStatus> {
        self.server.entity_authority_status(&self.entity)
    }
}

cfg_if! {
    if #[cfg(feature = "interior_visibility")] {
        
        use naia_shared::LocalEntity;

        use crate::UserKey;

        impl<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>> EntityRef<'s, E, W> {
            
            pub fn local_entity(&self, user_key: &UserKey) -> LocalEntity {
                self.server.world_to_local_entity(user_key, &self.entity)
            }
        }
    }
}