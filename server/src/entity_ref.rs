use std::hash::Hash;

use naia_shared::{
    ProtocolType, ReplicaMutWrapper, ReplicaRefWrapper, Replicate, ReplicateSafe, WorldMutType,
    WorldRefType,
};

use super::{room::room_key::RoomKey, server::Server, user::user_key::UserKey};

// EntityRef

/// A reference to an Entity being tracked by the Server
pub struct EntityRef<'s, P: ProtocolType, E: Copy + Eq + Hash, W: WorldRefType<P, E>> {
    server: &'s Server<P, E>,
    world: W,
    id: E,
}

impl<'s, P: ProtocolType, E: Copy + Eq + Hash, W: WorldRefType<P, E>> EntityRef<'s, P, E, W> {
    /// Return a new EntityRef
    pub(crate) fn new(server: &'s Server<P, E>, world: W, key: &E) -> Self {
        EntityRef {
            server,
            world,
            id: *key,
        }
    }

    /// Get the Entity's id
    pub fn id(&self) -> E {
        self.id
    }

    // Components

    /// Returns whether or not the Entity has an associated Component
    pub fn has_component<R: ReplicateSafe<P>>(&self) -> bool {
        return self.world.has_component::<R>(&self.id);
    }

    /// Gets a Ref to a Component associated with the Entity
    pub fn component<R: ReplicateSafe<P>>(&self) -> Option<ReplicaRefWrapper<P, R>> {
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
pub struct EntityMut<'s, P: ProtocolType, E: Copy + Eq + Hash, W: WorldMutType<P, E>> {
    server: &'s mut Server<P, E>,
    world: W,
    id: E,
}

impl<'s, P: ProtocolType, E: Copy + Eq + Hash, W: WorldMutType<P, E>> EntityMut<'s, P, E, W> {
    pub(crate) fn new(server: &'s mut Server<P, E>, world: W, key: &E) -> Self {
        EntityMut {
            server,
            world,
            id: *key,
        }
    }

    pub fn id(&self) -> E {
        self.id
    }

    pub fn despawn(&mut self) {
        self.server.despawn_entity(&mut self.world, &self.id);
    }

    // Components

    pub fn has_component<R: ReplicateSafe<P>>(&self) -> bool {
        return self.world.has_component::<R>(&self.id);
    }

    pub fn component<R: ReplicateSafe<P>>(&mut self) -> Option<ReplicaMutWrapper<P, R>> {
        return self.world.get_component_mut::<R>(&self.id);
    }

    pub fn insert_component<R: ReplicateSafe<P>>(&mut self, component_ref: R) -> &mut Self {
        self.server
            .insert_component(&mut self.world, &self.id, component_ref);

        self
    }

    pub fn insert_components<R: ReplicateSafe<P>>(
        &mut self,
        mut component_refs: Vec<R>,
    ) -> &mut Self {
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
pub struct WorldlessEntityMut<'s, P: ProtocolType, E: Copy + Eq + Hash> {
    server: &'s mut Server<P, E>,
    id: E,
}

impl<'s, P: ProtocolType, E: Copy + Eq + Hash> WorldlessEntityMut<'s, P, E> {
    pub(crate) fn new(server: &'s mut Server<P, E>, key: &E) -> Self {
        WorldlessEntityMut { server, id: *key }
    }

    pub fn id(&self) -> E {
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
