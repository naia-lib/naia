use bevy_ecs::entity::Entity;

use naia_bevy_shared::{Channel, Message};
use naia_server::{RoomRef as InnerRoomRef, RoomMut as InnerRoomMut, RoomKey, UserKey};

use crate::{world_entity::WorldEntity, WorldId};

//// RoomRef ////

pub struct RoomRef<'a> {
    world_id: WorldId,
    inner: InnerRoomRef<'a, WorldEntity>,
}

impl<'a> RoomRef<'a> {
    pub(crate) fn new(world_id: WorldId, inner: InnerRoomRef<'a, WorldEntity>) -> Self {
        Self { world_id, inner }
    }

    pub fn key(&self) -> RoomKey {
        self.inner.key()
    }

    // Users

    pub fn has_user(&self, user_key: &UserKey) -> bool {
        self.inner.has_user(user_key)
    }

    pub fn users_count(&self) -> usize {
        self.inner.users_count()
    }

    /// Returns an iterator of the [`UserKey`] for Users that belong in the [`Room`]
    pub fn user_keys(&self) -> impl Iterator<Item = &UserKey> {
        self.inner.user_keys()
    }

    // Entities

    pub fn has_entity(&self, entity: &Entity) -> bool {
        let world_entity = WorldEntity::new(self.world_id, *entity);
        self.inner.has_entity(&world_entity)
    }

    pub fn entities_count(&self) -> usize {
        self.inner.entities_count()
    }

    pub fn entities(&self) -> Vec<Entity> {
        self.inner.entities().iter().map(|world_entity| world_entity.entity()).collect()
    }
}

//// RoomMut ////

pub struct RoomMut<'a> {
    world_id: WorldId,
    inner: InnerRoomMut<'a, WorldEntity>,
}

impl<'a> RoomMut<'a> {
    pub(crate) fn new(world_id: WorldId, inner: InnerRoomMut<'a, WorldEntity>) -> Self {
        Self { world_id, inner }
    }

    pub fn key(&self) -> RoomKey {
        self.inner.key()
    }

    pub fn destroy(&mut self) {
        self.inner.destroy();
    }

    // Users

    pub fn has_user(&self, user_key: &UserKey) -> bool {
        self.inner.has_user(user_key)
    }

    pub fn add_user(&mut self, user_key: &UserKey) -> &mut Self {
        self.inner.add_user(user_key);

        self
    }

    pub fn remove_user(&mut self, user_key: &UserKey) -> &mut Self {
        self.inner.remove_user(user_key);

        self
    }

    pub fn users_count(&self) -> usize {
        self.inner.users_count()
    }

    /// Returns an iterator of the [`UserKey`] for Users that belong in the [`Room`]
    pub fn user_keys(&self) -> impl Iterator<Item = &UserKey> {
        self.inner.user_keys()
    }

    // Entities

    pub fn has_entity(&self, entity: &Entity) -> bool {
        let world_entity = WorldEntity::new(self.world_id, *entity);
        self.inner.has_entity(&world_entity)
    }

    pub fn add_entity(&mut self, entity: &Entity) -> &mut Self {
        let world_entity = WorldEntity::new(self.world_id, *entity);
        self.inner.add_entity(&world_entity);

        self
    }

    pub fn remove_entity(&mut self, entity: &Entity) -> &mut Self {
        let world_entity = WorldEntity::new(self.world_id, *entity);
        self.inner.remove_entity(&world_entity);

        self
    }

    pub fn entities_count(&self) -> usize {
        self.inner.entities_count()
    }

    pub fn broadcast_message<C: Channel, M: Message>(&mut self, message: &M) {
        self.inner.broadcast_message::<C, M>(message);
    }
}