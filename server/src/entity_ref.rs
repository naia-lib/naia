use std::any::TypeId;

use naia_shared::{ImplRef, ProtocolType, Ref, Replicate};

use crate::{EntityKey, RoomKey, Server, UserKey};

// EntityRef

/// A reference to an Entity being tracked by the Server
pub struct EntityRef<'s, P: ProtocolType> {
    server: &'s Server<P>,
    key: EntityKey,
}

impl<'s, P: ProtocolType> EntityRef<'s, P> {
    /// Return a new EntityRef
    pub fn new(server: &'s Server<P>, key: &EntityKey) -> Self {
        EntityRef { server, key: *key }
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

    pub fn has_owner(&self) -> bool {
        return self.server.entity_has_owner(&self.key);
    }

    pub fn get_owner(&self) -> Option<&UserKey> {
        return self.server.entity_get_owner(&self.key);
    }
}

// EntityMut
pub struct EntityMut<'s, P: ProtocolType> {
    server: &'s mut Server<P>,
    key: EntityKey,
}

impl<'s, P: ProtocolType> EntityMut<'s, P> {
    pub fn new(server: &'s mut Server<P>, key: &EntityKey) -> Self {
        EntityMut { server, key: *key }
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
