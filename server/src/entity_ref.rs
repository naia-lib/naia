use std::any::TypeId;

use naia_shared::{ImplRef, ProtocolType, Ref, Replicate};

use crate::{EntityKey, RoomKey, Server, UserKey};

// EntityRef

pub struct EntityRef<'s, P: ProtocolType> {
    server: &'s Server<P>,
    key: EntityKey,
}

impl<'s, P: ProtocolType> EntityRef<'s, P> {
    pub fn new(server: &'s Server<P>, key: &EntityKey) -> Self {
        EntityRef { server, key: *key }
    }

    pub fn key(&self) -> EntityKey {
        self.key
    }

    pub fn has_component<R: Replicate<P>>(&self) -> bool {
        return self
            .server
            .entity_contains_type_id(&self.key, &TypeId::of::<R>());
    }

    pub fn component<R: Replicate<P>>(&self) -> Option<&Ref<R>> {
        if let Some(protocol) = self.server.get_component_by_type::<R>(&self.key) {
            return protocol.as_typed_ref::<R>();
        }
        return None;
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

    pub fn has_component<R: Replicate<P>>(&self) -> bool {
        return self
            .server
            .entity_contains_type_id(&self.key, &TypeId::of::<R>());
    }

    pub fn component<R: Replicate<P>>(&self) -> Option<&Ref<R>> {
        if let Some(protocol) = self.server.get_component_by_type::<R>(&self.key) {
            return protocol.as_typed_ref::<R>();
        }
        return None;
    }

    // Add Component
    pub fn insert_component<R: ImplRef<P>>(&mut self, component_ref: &R) -> &mut Self {
        self.server
            .insert_component_to_entity(&self.key, component_ref);

        self
    }

    pub fn insert_components<R: ImplRef<P>>(&mut self, component_refs: &[R]) -> &mut Self {
        for component_ref in component_refs {
            self.server
                .insert_component_to_entity(&self.key, component_ref);
        }

        self
    }

    // Remove Component
    pub fn remove_component<R: Replicate<P>>(&mut self) -> &mut Self {
        self.server.remove_component_by_type::<R>(&self.key);

        self
    }

    // Despawn Entity
    pub fn despawn(&mut self) {
        self.server.despawn_entity(&self.key);
    }

    // Pawns & Users

    pub fn assign_user(&mut self, user_key: &UserKey) -> &mut Self {
        self.server.assign_pawn_entity(user_key, &self.key);

        self
    }

    pub fn unassign_user(&mut self, user_key: &UserKey) -> &mut Self {
        self.server.unassign_pawn_entity(user_key, &self.key);

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
