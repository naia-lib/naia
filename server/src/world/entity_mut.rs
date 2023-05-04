use std::hash::Hash;

use naia_shared::{ReplicaMutWrapper, Replicate, WorldMutType};

use crate::{room::RoomKey, server::Server, ReplicationConfig};

// EntityMut
pub struct EntityMut<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>> {
    server: &'s mut Server<E>,
    world: W,
    entity: E,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>> EntityMut<'s, E, W> {
    pub(crate) fn new(server: &'s mut Server<E>, world: W, entity: &E) -> Self {
        EntityMut {
            server,
            world,
            entity: *entity,
        }
    }

    pub fn id(&self) -> E {
        self.entity
    }

    pub fn despawn(&mut self) {
        self.server.despawn_entity(&mut self.world, &self.entity);
    }

    // Components

    pub fn has_component<R: Replicate>(&self) -> bool {
        self.world.has_component::<R>(&self.entity)
    }

    pub fn component<R: Replicate>(&mut self) -> Option<ReplicaMutWrapper<R>> {
        self.world.component_mut::<R>(&self.entity)
    }

    pub fn insert_component<R: Replicate>(&mut self, component_ref: R) -> &mut Self {
        self.server
            .insert_component(&mut self.world, &self.entity, component_ref);

        self
    }

    pub fn insert_components<R: Replicate>(&mut self, mut component_refs: Vec<R>) -> &mut Self {
        while let Some(component_ref) = component_refs.pop() {
            self.insert_component(component_ref);
        }

        self
    }

    pub fn remove_component<R: Replicate>(&mut self) -> Option<R> {
        self.server
            .remove_component::<R, W>(&mut self.world, &self.entity)
    }

    // Authority / Config

    pub fn configure_replication(&mut self, config: ReplicationConfig) -> &mut Self {
        self.server
            .configure_entity_replication(&mut self.world, &self.entity, config);

        self
    }

    pub fn replication_config(&self) -> Option<ReplicationConfig> {
        self.server.entity_replication_config(&self.entity)
    }

    pub fn has_authority(&self) -> bool {
        todo!();
        //self.server.entity_has_authority(&self.entity)
    }

    pub fn request_authority(&mut self) -> &mut Self {
        todo!();
        //self.server.request_entity_authority(&self.entity);

        self
    }

    pub fn release_authority(&mut self) -> &mut Self {
        todo!();
        //self.server.release_entity_authority(&self.entity);

        self
    }

    // Rooms

    pub fn enter_room(&mut self, room_key: &RoomKey) -> &mut Self {
        self.server.room_add_entity(room_key, &self.entity);

        self
    }

    pub fn leave_room(&mut self, room_key: &RoomKey) -> &mut Self {
        self.server.room_remove_entity(room_key, &self.entity);

        self
    }
}
