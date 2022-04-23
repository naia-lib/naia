use bevy_ecs::entity::Entity;

use naia_server::{
    shared::{ChannelIndex, Protocolize, Replicate, ReplicateSafe},
    RoomKey,
};

use super::{
    commands::{DespawnEntity, InsertComponent, RemoveComponent},
    server::Server,
};

// EntityMut

pub struct EntityMut<'s, 'world, 'state, P: Protocolize, C: ChannelIndex> {
    entity: Entity,
    server: &'s mut Server<'world, 'state, P, C>,
}

impl<'s, 'world, 'state, P: Protocolize, C: ChannelIndex> EntityMut<'s, 'world, 'state, P, C> {
    pub fn new(entity: Entity, server: &'s mut Server<'world, 'state, P, C>) -> Self {
        EntityMut { entity, server }
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

    pub fn insert<R: ReplicateSafe<P>>(&mut self, component: R) -> &mut Self {
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

    pub fn server(&mut self) -> &mut Server<'world, 'state, P, C> {
        self.server
    }
}
