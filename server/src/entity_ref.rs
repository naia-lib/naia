use naia_shared::{EntityType, ProtocolType, ReplicateSafe, WorldMutType, WorldRefType, ComponentRef, ComponentMut, Replicate};

use super::{room::room_key::RoomKey, server::Server, user::user_key::UserKey};

// EntityRef

/// A reference to an Entity being tracked by the Server
pub struct EntityRef<'s, P: ProtocolType, K: EntityType, W: WorldRefType<P, K>> {
    server: &'s Server<P, K>,
    world: W,
    id: K,
}

impl<'s, P: ProtocolType, K: EntityType, W: WorldRefType<P, K>> EntityRef<'s, P, K, W> {
    /// Return a new EntityRef
    pub(crate) fn new(server: &'s Server<P, K>, world: W, key: &K) -> Self {
        EntityRef {
            server,
            world,
            id: *key,
        }
    }

    /// Get the Entity's id
    pub fn id(&self) -> K {
        self.id
    }

    // Components

    /// Returns whether or not the Entity has an associated Component
    pub fn has_component<R: ReplicateSafe<P>>(&self) -> bool {
        return self.world.has_component::<R>(&self.id);
    }

    /// Gets a Ref to a Component associated with the Entity
    pub fn component<R: ReplicateSafe<P>>(&self) -> Option<ComponentRef<P, R>> {
        return self.world.get_component::<R>(&self.id);
    }

    // Ownership

    /// Returns whether or not the Entity is owned/controlled by a User
    pub fn has_owner(&self) -> bool {
        return self.server.entity_has_owner(&self.id);
    }

    /// Returns the UserKey associated with the Entity's owner/controller
    pub fn get_owner(&self) -> Option<&UserKey> {
        return self.server.entity_get_owner(&self.id);
    }
}

// EntityMut
pub struct EntityMut<'s, P: ProtocolType, K: EntityType, W: WorldMutType<P, K>> {
    server: &'s mut Server<P, K>,
    world: W,
    id: K,
}

impl<'s, P: ProtocolType, K: EntityType, W: WorldMutType<P, K>> EntityMut<'s, P, K, W> {
    pub(crate) fn new(server: &'s mut Server<P, K>, world: W, key: &K) -> Self {
        EntityMut {
            server,
            world,
            id: *key,
        }
    }

    pub fn id(&self) -> K {
        self.id
    }

    pub fn despawn(&mut self) {
        self.server.despawn_entity(&mut self.world, &self.id);
    }

    // Components

    pub fn has_component<R: ReplicateSafe<P>>(&self) -> bool {
        return self.world.has_component::<R>(&self.id);
    }

    pub fn component<R: ReplicateSafe<P>>(&mut self) -> Option<ComponentMut<P, R>> {
        return self.world.get_component_mut::<R>(&self.id);
    }

    pub fn insert_component<R: ReplicateSafe<P>>(&mut self, component_ref: R) -> &mut Self {
        self.server
            .insert_component(&mut self.world, &self.id, component_ref);

        self
    }

    pub fn insert_components<R: ReplicateSafe<P>>(&mut self, mut component_refs: Vec<R>) -> &mut Self {
        while let Some(component_ref) = component_refs.pop() {
            self.insert_component(component_ref);
        }

        self
    }

    pub fn remove_component<R: Replicate<P>>(&mut self) -> Option<R> {
        return self
            .server
            .remove_component::<R, W>(&mut self.world, &self.id);
    }

    // Users & Assignment

    pub fn has_owner(&self) -> bool {
        return self.server.entity_has_owner(&self.id);
    }

    pub fn get_owner(&self) -> Option<&UserKey> {
        return self.server.entity_get_owner(&self.id);
    }

    pub fn set_owner(&mut self, user_key: &UserKey) -> &mut Self {
        // user_own?
        self.server.entity_set_owner(&self.id, user_key);

        self
    }

    pub fn disown(&mut self) -> &mut Self {
        self.server.entity_disown(&self.id);

        self
    }

    // Rooms

    pub fn enter_room(&mut self, room_key: &RoomKey) -> &mut Self {
        self.server.room_add_entity(room_key, &self.id);

        self
    }

    pub fn leave_room(&mut self, room_key: &RoomKey) -> &mut Self {
        self.server.room_remove_entity(room_key, &self.id);

        self
    }
}

// WorldlessEntityMut
pub struct WorldlessEntityMut<'s, P: ProtocolType, K: EntityType> {
    server: &'s mut Server<P, K>,
    id: K,
}

impl<'s, P: ProtocolType, K: EntityType> WorldlessEntityMut<'s, P, K> {
    pub(crate) fn new(server: &'s mut Server<P, K>, key: &K) -> Self {
        WorldlessEntityMut { server, id: *key }
    }

    pub fn id(&self) -> K {
        self.id
    }

    // Users & Assignment

    pub fn has_owner(&self) -> bool {
        return self.server.entity_has_owner(&self.id);
    }

    pub fn get_owner(&self) -> Option<&UserKey> {
        return self.server.entity_get_owner(&self.id);
    }

    pub fn disown(&mut self) -> &mut Self {
        self.server.entity_disown(&self.id);
        self
    }

    // Rooms

    pub fn enter_room(&mut self, room_key: &RoomKey) -> &mut Self {
        self.server.room_add_entity(room_key, &self.id);
        self
    }

    pub fn leave_room(&mut self, room_key: &RoomKey) -> &mut Self {
        self.server.room_remove_entity(room_key, &self.id);
        self
    }
}
