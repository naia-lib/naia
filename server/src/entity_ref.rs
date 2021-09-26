use std::any::TypeId;

use naia_shared::{ImplRef, ProtocolType, Ref, Replicate};

use crate::{EntityKey, RoomKey, Server, UserKey, WorldType};

// EntityRef

/// A reference to an Entity being tracked by the Server
pub struct EntityRef<'s, 'w, P: ProtocolType, W: WorldType> {
    server: &'s Server<P>,
    world: &'w W,
    key: EntityKey,
}

impl<'s, 'w, P: ProtocolType, W: WorldType> EntityRef<'s, 'w, P, W> {
    /// Return a new EntityRef
    pub(crate) fn new(server: &'s Server<P>, world: &'w W, key: &EntityKey) -> Self {
        EntityRef { server, world, key: *key }
    }

    /// Gets the EntityKey associated with the Entity
    pub fn key(&self) -> EntityKey {
        self.key
    }

    // Components

    /// Returns whether or not the Entity has an associated Component
    pub fn has_component<R: Replicate<P>>(&self) -> bool {
        return self
            .server
            .entity_contains_type_id(&self.key, &TypeId::of::<R>());
    }

    /// Gets a Ref to a Component associated with the Entity
    pub fn component<R: Replicate<P>>(&self) -> Option<&Ref<R>> {
        return self.server.component::<R>(&self.key);
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
pub struct EntityMut<'s, 'w, P: ProtocolType, W: WorldType> {
    server: &'s mut Server<P>,
    world: &'w mut W,
    key: EntityKey,
}

impl<'s, 'w, P: ProtocolType, W: WorldType> EntityMut<'s, 'w, P, W> {
    pub(crate) fn new(server: &'s mut Server<P>, world: &'w mut W, key: &EntityKey) -> Self {
        EntityMut { server, world, key: *key }
    }

    pub fn key(&self) -> EntityKey {
        self.key
    }

    pub fn despawn(&mut self) {
        self.server.despawn_entity(&self.key);
    }

    // Components

    pub fn has_component<R: Replicate<P>>(&self) -> bool {
        return self
            .server
            .entity_contains_type_id(&self.key, &TypeId::of::<R>());
    }

    pub fn component<R: Replicate<P>>(&self) -> Option<&Ref<R>> {
        return self.server.component::<R>(&self.key);
    }

    pub fn insert_component<R: ImplRef<P>>(&mut self, component_ref: &R) -> &mut Self {

        let new_ref = component_ref.clone_ref();
        self.world.insert_component(&self.key, new_ref);
        self.server.insert_component(&self.key, component_ref);

        self
    }

    pub fn insert_components<R: ImplRef<P>>(&mut self, component_refs: &[R]) -> &mut Self {
        self.server.insert_components(&self.key, component_refs);

        self
    }

    pub fn remove_component<R: Replicate<P>>(&mut self) -> Option<Ref<R>> {
        return self.server.remove_component::<R>(&self.key);
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
        self.server.entity_set_owner(&self.key, user_key);

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
