use bevy::ecs::entity::Entity;

use naia_server::{
    shared::{Protocolize, Replicate},
    RoomKey,
};

use super::{
    commands::{DespawnEntity, InsertComponent, RemoveComponent},
    server::Server,
};

// EntityMut

pub struct EntityMut<'s, 'world, 'state, P: Protocolize> {
    entity: Entity,
    server: &'s mut Server<'world, 'state, P>,
}

impl<'s, 'world, 'state, P: Protocolize> EntityMut<'s, 'world, 'state, P> {
    pub fn new(entity: Entity, server: &'s mut Server<'world, 'state, P>) -> Self {
        return EntityMut { entity, server };
    }

    #[inline]
    pub fn id(&self) -> Entity {
        self.entity
    }

    // Despawn

    pub fn despawn(&mut self) {
        self.server.queue_command(DespawnEntity::new(&self.entity))
    }

    // Components

    pub fn insert<R: Replicate<P>>(&mut self, component: R) -> &mut Self {
        self.server
            .queue_command(InsertComponent::new(&self.entity, component));
        self
    }

    pub fn remove<R: Replicate<P>>(&mut self) -> &mut Self {
        self.server
            .queue_command(RemoveComponent::<P, R>::new(&self.entity));
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

    // Exit

    pub fn server(&mut self) -> &mut Server<'world, 'state, P> {
        self.server
    }
}
