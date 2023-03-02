use bevy_ecs::entity::Entity;

use naia_bevy_shared::Replicate;

use super::{
    client::Client,
    commands::{DespawnEntity, InsertComponent, RemoveComponent},
};

// EntityMut

pub struct EntityMut<'s, 'world, 'state> {
    entity: Entity,
    client: &'s mut Client<'world, 'state>,
}

impl<'s, 'world, 'state> EntityMut<'s, 'world, 'state> {
    pub fn new(entity: Entity, client: &'s mut Client<'world, 'state>) -> Self {
        EntityMut { entity, client }
    }

    #[inline]
    pub fn id(&self) -> Entity {
        self.entity
    }

    // Despawn

    pub fn despawn(&mut self) {
        self.client.queue_command(DespawnEntity::new(&self.entity))
    }

    // Components

    pub fn insert<R: Replicate>(&mut self, component: R) -> &mut Self {
        self.client
            .queue_command(InsertComponent::new(&self.entity, component));
        self
    }

    pub fn remove<R: Replicate>(&mut self) -> &mut Self {
        self.client
            .queue_command(RemoveComponent::<R>::new(&self.entity));
        self
    }
}
