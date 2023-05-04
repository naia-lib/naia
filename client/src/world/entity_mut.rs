use std::hash::Hash;

use naia_shared::{EntityAuthStatus, ReplicaMutWrapper, Replicate, WorldMutType};

use crate::{Client, ReplicationConfig};

// EntityMut
pub struct EntityMut<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>> {
    client: &'s mut Client<E>,
    world: W,
    entity: E,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>> EntityMut<'s, E, W> {
    pub(crate) fn new(client: &'s mut Client<E>, world: W, entity: &E) -> Self {
        EntityMut {
            client,
            world,
            entity: *entity,
        }
    }

    pub fn id(&self) -> E {
        self.entity
    }

    pub fn despawn(&mut self) {
        self.client.despawn_entity(&mut self.world, &self.entity);
    }

    // Components

    pub fn has_component<R: Replicate>(&self) -> bool {
        self.world.has_component::<R>(&self.entity)
    }

    pub fn component<R: Replicate>(&mut self) -> Option<ReplicaMutWrapper<R>> {
        self.world.component_mut::<R>(&self.entity)
    }

    pub fn insert_component<R: Replicate>(&mut self, component_ref: R) -> &mut Self {
        self.client
            .insert_component(&mut self.world, &self.entity, component_ref);

        self
    }

    pub fn remove_component<R: Replicate>(&mut self) -> Option<R> {
        self.client
            .remove_component::<R, W>(&mut self.world, &self.entity)
    }

    // Authority / Config

    pub fn configure_replication(&mut self, config: ReplicationConfig) -> &mut Self {
        self.client
            .configure_entity_replication(&mut self.world, &self.entity, config);

        self
    }

    pub fn replication_config(&self) -> Option<ReplicationConfig> {
        self.client.entity_replication_config(&self.entity)
    }

    pub fn authority(&self) -> EntityAuthStatus {
        self.client.entity_authority_status(&self.entity)
    }

    pub fn request_authority(&mut self) -> &mut Self {
        self.client.entity_request_authority(&self.entity);

        self
    }

    pub fn release_authority(&mut self) -> &mut Self {
        self.client.entity_release_authority(&self.entity);

        self
    }
}
