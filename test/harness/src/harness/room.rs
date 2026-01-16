use naia_server::{RoomKey, RoomMut as NaiaRoomMut, RoomRef as NaiaRoomRef};
use naia_shared::{Channel, Message};

use crate::{
    harness::{entity_registry::EntityRegistry, users::Users, ClientKey, EntityKey},
    TestEntity,
};

/// Harness wrapper for RoomRef that works with EntityKey/ClientKey instead of TestEntity/UserKey
pub struct RoomRef<'a> {
    room: NaiaRoomRef<'a, TestEntity>,
    registry: &'a EntityRegistry,
    users: Users<'a>,
}

impl<'a> RoomRef<'a> {
    pub(crate) fn new(
        room: NaiaRoomRef<'a, TestEntity>,
        registry: &'a EntityRegistry,
        users: Users<'a>,
    ) -> Self {
        Self {
            room,
            registry,
            users,
        }
    }

    /// Get the RoomKey for this room
    pub fn key(&self) -> RoomKey {
        self.room.key()
    }

    /// Check if a user (by ClientKey) is in this room
    pub fn has_user(&self, client_key: &ClientKey) -> bool {
        if let Some(user_key) = self.users.client_to_user_key(client_key) {
            self.room.has_user(&user_key)
        } else {
            false
        }
    }

    /// Get the number of users in this room
    pub fn users_count(&self) -> usize {
        self.room.users_count()
    }

    /// Get all user keys (as ClientKeys) in this room
    pub fn user_keys(&self) -> Vec<ClientKey> {
        self.room
            .user_keys()
            .filter_map(|uk| self.users.user_to_client_key(uk))
            .collect()
    }

    /// Check if an entity (by EntityKey) is in this room
    pub fn has_entity(&self, entity_key: &EntityKey) -> bool {
        if let Some(entity) = self.registry.server_entity(entity_key) {
            self.room.has_entity(&entity)
        } else {
            false
        }
    }

    /// Get the number of entities in this room
    pub fn entities_count(&self) -> usize {
        self.room.entities_count()
    }

    /// Get all entities (as EntityKeys) in this room
    pub fn entities(&self) -> Vec<EntityKey> {
        self.room
            .entities()
            .iter()
            .filter_map(|entity| self.registry.entity_key_for_server_entity(entity))
            .collect()
    }
}

/// Harness wrapper for RoomMut that works with EntityKey/ClientKey instead of TestEntity/UserKey
pub struct RoomMut<'a> {
    room: NaiaRoomMut<'a, TestEntity>,
    registry: &'a EntityRegistry,
    users: Users<'a>,
}

impl<'a> RoomMut<'a> {
    pub(crate) fn new(
        room: NaiaRoomMut<'a, TestEntity>,
        registry: &'a EntityRegistry,
        users: Users<'a>,
    ) -> Self {
        Self {
            room,
            registry,
            users,
        }
    }

    /// Get the RoomKey for this room
    pub fn key(&self) -> RoomKey {
        self.room.key()
    }

    /// Destroy this room
    pub fn destroy(&mut self) {
        self.room.destroy();
    }

    /// Check if a user (by ClientKey) is in this room
    pub fn has_user(&self, client_key: &ClientKey) -> bool {
        if let Some(user_key) = self.users.client_to_user_key(client_key) {
            self.room.has_user(&user_key)
        } else {
            false
        }
    }

    /// Add a user (by ClientKey) to this room
    pub fn add_user(&mut self, client_key: &ClientKey) -> &mut Self {
        if let Some(user_key) = self.users.client_to_user_key(client_key) {
            self.room.add_user(&user_key);
        }
        self
    }

    /// Remove a user (by ClientKey) from this room
    pub fn remove_user(&mut self, client_key: &ClientKey) -> &mut Self {
        if let Some(user_key) = self.users.client_to_user_key(client_key) {
            self.room.remove_user(&user_key);
        }
        self
    }

    /// Get the number of users in this room
    pub fn users_count(&self) -> usize {
        self.room.users_count()
    }

    /// Get all user keys (as ClientKeys) in this room
    pub fn user_keys(&self) -> Vec<ClientKey> {
        self.room
            .user_keys()
            .filter_map(|uk| self.users.user_to_client_key(uk))
            .collect()
    }

    /// Check if an entity (by EntityKey) is in this room
    pub fn has_entity(&self, entity_key: &EntityKey) -> bool {
        if let Some(entity) = self.registry.server_entity(entity_key) {
            self.room.has_entity(&entity)
        } else {
            false
        }
    }

    /// Add an entity (by EntityKey) to this room
    pub fn add_entity(&mut self, entity_key: &EntityKey) -> &mut Self {
        if let Some(entity) = self.registry.server_entity(entity_key) {
            self.room.add_entity(&entity);
        }
        self
    }

    /// Remove an entity (by EntityKey) from this room
    pub fn remove_entity(&mut self, entity_key: &EntityKey) -> &mut Self {
        if let Some(entity) = self.registry.server_entity(entity_key) {
            self.room.remove_entity(&entity);
        }
        self
    }

    /// Get the number of entities in this room
    pub fn entities_count(&self) -> usize {
        self.room.entities_count()
    }

    /// Broadcast a message to all users in this room
    pub fn broadcast_message<C: Channel, M: Message>(&mut self, message: &M) {
        self.room.broadcast_message::<C, M>(message);
    }
}
