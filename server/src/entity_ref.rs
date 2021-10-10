use naia_shared::{ImplRef, ProtocolType, Ref, Replicate};

use super::{
    keys::EntityType, room::room_key::RoomKey, server::Server, user::user_key::UserKey,
    world_type::{WorldRefType, WorldMutType},
};

// EntityRef

/// A reference to an Entity being tracked by the Server
pub struct EntityRef<'s, P: ProtocolType, K: EntityType, W: WorldRefType<P, K>> {
    server: &'s Server<P, K>,
    world: W,
    key: K,
}

impl<'s, P: ProtocolType, K: EntityType, W: WorldRefType<P, K>> EntityRef<'s, P, K, W> {
    /// Return a new EntityRef
    pub(crate) fn new(server: &'s Server<P, K>, world: W, key: &K) -> Self {
        EntityRef {
            server,
            world,
            key: *key,
        }
    }

    /// Gets the Key associated with the Entity
    pub fn key(&self) -> K {
        self.key
    }

    // Components

    /// Returns whether or not the Entity has an associated Component
    pub fn has_component<R: Replicate<P>>(&self) -> bool {
        return self.world.has_component::<R>(&self.key);
    }

    /// Gets a Ref to a Component associated with the Entity
    pub fn component<R: Replicate<P>>(&self) -> Option<Ref<R>> {
        return self.world.get_component::<R>(&self.key);
    }

    // Ownership

    /// Returns whether or not the Entity is owned/controlled by a User
    pub fn has_owner(&self) -> bool {
        return self.server.entity_has_owner(&self.key);
    }

    /// Returns the UserKey associated with the Entity's owner/controller
    pub fn get_owner(&self) -> Option<&UserKey> {
        return self.server.entity_get_owner(&self.key);
    }
}

// EntityMut
pub struct EntityMut<'s, 'w,  P: ProtocolType, K: EntityType, W: WorldMutType<P, K>> {
    server: &'s mut Server<P, K>,
    world: &'w mut W,
    key: K,
}

impl<'s, 'w, P: ProtocolType, K: EntityType, W: WorldMutType<P, K>> EntityMut<'s,'w, P, K, W> {
    pub(crate) fn new(server: &'s mut Server<P, K>, world: &'w mut W, key: &K) -> Self {
        EntityMut {
            server,
            world,
            key: *key,
        }
    }

    pub fn key(&self) -> K {
        self.key
    }

    pub fn despawn(&mut self) {
        self.server.despawn_entity(self.world, &self.key);
    }

    // Components

    pub fn has_component<R: Replicate<P>>(&self) -> bool {
        return self.world.has_component::<R>(&self.key);
    }

    pub fn component<R: Replicate<P>>(&self) -> Option<Ref<R>> {
        return self.world.get_component::<R>(&self.key);
    }

    pub fn insert_component<R: ImplRef<P>>(&mut self, component_ref: &R) -> &mut Self {
        self.server
            .insert_component(self.world, &self.key, component_ref);

        self
    }

    pub fn insert_components<R: ImplRef<P>>(&mut self, component_refs: &[R]) -> &mut Self {
        for component_ref in component_refs {
            self.insert_component(component_ref);
        }

        self
    }

    pub fn remove_component<R: Replicate<P>>(&mut self) -> Option<Ref<R>> {
        return self
            .server
            .remove_component::<R, W>(&mut self.world, &self.key);
    }

    // Users & Assignment

    pub fn has_owner(&self) -> bool {
        return self.server.entity_has_owner(&self.key);
    }

    pub fn get_owner(&self) -> Option<&UserKey> {
        return self.server.entity_get_owner(&self.key);
    }

    pub fn set_owner(&mut self, user_key: &UserKey) -> &mut Self {
        // user_own?
        self.server
            .entity_set_owner(self.world, &self.key, user_key);

        self
    }

    pub fn disown(&mut self) -> &mut Self {
        self.server.entity_disown(&self.key);

        self
    }

    // Rooms

    pub fn enter_room(&mut self, room_key: &RoomKey) -> &mut Self {
        self.server.room_add_entity(room_key, &self.key);

        self
    }

    pub fn leave_room(&mut self, room_key: &RoomKey) -> &mut Self {
        self.server.room_remove_entity(room_key, &self.key);

        self
    }
}

// WorldlessEntityMut
pub struct WorldlessEntityMut<'s, P: ProtocolType, K: EntityType> {
    server: &'s mut Server<P, K>,
    key: K,
}

impl<'s, P: ProtocolType, K: EntityType> WorldlessEntityMut<'s, P, K> {
    pub(crate) fn new(server: &'s mut Server<P, K>, key: &K) -> Self {
        WorldlessEntityMut {
            server,
            key: *key,
        }
    }

    pub fn disown(&mut self) -> &mut Self {
        self.server.entity_disown(&self.key);

        self
    }
}
