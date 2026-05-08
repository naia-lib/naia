use std::hash::Hash;

use naia_shared::{
    AuthorityError, EntityAuthStatus, ReplicaMutWrapper, ReplicatedComponent, WorldMutType,
};

use crate::{room::RoomKey, server::WorldServer, EntityOwner, ReplicationConfig, UserKey};

// EntityMut
pub struct EntityMut<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>> {
    server: &'s mut WorldServer<E>,
    world: W,
    entity: E,
    /// True after `as_static()` is called — allows component insertion during
    /// construction. False for all other sources where mutation of a static
    /// entity after construction is illegal.
    allow_static_insert: bool,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>> EntityMut<'s, E, W> {
    pub(crate) fn new(server: &'s mut WorldServer<E>, world: W, entity: &E) -> Self {
        Self {
            server,
            world,
            entity: *entity,
            allow_static_insert: false,
        }
    }

    pub fn id(&self) -> E {
        self.entity
    }

    /// Mark this entity as static: no diff-tracking after initial replication.
    /// Must be called before inserting components; returns `&mut Self` for chaining.
    pub fn as_static(&mut self) -> &mut Self {
        self.server.mark_entity_as_static(&self.entity);
        self.allow_static_insert = true;
        self
    }

    pub fn despawn(&mut self) {
        self.server.despawn_entity(&mut self.world, &self.entity);
    }

    // Components

    pub fn has_component<R: ReplicatedComponent>(&self) -> bool {
        self.world.has_component::<R>(&self.entity)
    }

    pub fn component<R: ReplicatedComponent>(&'_ mut self) -> Option<ReplicaMutWrapper<'_, R>> {
        self.world.component_mut::<R>(&self.entity)
    }

    pub fn insert_component<R: ReplicatedComponent>(&mut self, component_ref: R) -> &mut Self {
        if !self.allow_static_insert && self.server.entity_is_static(&self.entity) {
            panic!("Cannot insert_component on a static entity after construction: call .as_static() and insert all components before dropping EntityMut");
        }
        self.server
            .insert_component(&mut self.world, &self.entity, component_ref);

        self
    }

    pub fn remove_component<R: ReplicatedComponent>(&mut self) -> Option<R> {
        if self.server.entity_is_static(&self.entity) {
            panic!("Cannot remove_component on a static entity"); // no allow_static_insert exception — removal is never valid
        }
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

    pub fn authority(&self) -> Option<EntityAuthStatus> {
        self.server.entity_authority_status(&self.entity)
    }

    pub fn owner(&self) -> EntityOwner {
        self.server.entity_owner(&self.entity)
    }

    pub fn give_authority(&mut self, user_key: &UserKey) -> Result<&mut Self, AuthorityError> {
        self.server.entity_give_authority(user_key, &self.entity)?;
        Ok(self)
    }

    pub fn take_authority(&mut self) -> Result<&mut Self, AuthorityError> {
        self.server.entity_take_authority(&self.entity)?;
        Ok(self)
    }

    pub fn release_authority(&mut self) -> Result<&mut Self, AuthorityError> {
        self.server.entity_release_authority(None, &self.entity)?;
        Ok(self)
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

cfg_if! {
    if #[cfg(feature = "interior_visibility")] {

        use naia_shared::LocalEntity;

        impl<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>> EntityMut<'s, E, W> {

            pub fn local_entity(&self, user_key: &UserKey) -> Option<LocalEntity> {
                self.server.world_to_local_entity(user_key, &self.entity)
            }
        }
    }
}
