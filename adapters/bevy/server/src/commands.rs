use std::marker::PhantomData;

use bevy_ecs::entity::Entity;

use naia_server::{
    shared::Replicate,
    Server,
};

use naia_bevy_shared::WorldMut;

// Command Trait

pub trait Command: Send + Sync + 'static {
    fn write(self: Box<Self>, server: &mut Server<Entity>, world: WorldMut);
}

//// Despawn Entity ////

pub(crate) struct DespawnEntity {
    entity: Entity,
}

impl DespawnEntity {
    pub fn new(entity: &Entity) -> Self {
        DespawnEntity { entity: *entity }
    }
}

impl Command for DespawnEntity {
    fn write(self: Box<Self>, server: &mut Server<Entity>, world: WorldMut) {
        server.entity_mut(world, &self.entity).despawn();
    }
}

//// Insert Component ////

pub(crate) struct InsertComponent<R: Replicate> {
    entity: Entity,
    component: R,
}

impl<R: Replicate> InsertComponent<R> {
    pub fn new(entity: &Entity, component: R) -> Self {
        InsertComponent {
            entity: *entity,
            component,
        }
    }
}

impl<R: Replicate> Command for InsertComponent<R> {
    fn write(self: Box<Self>, server: &mut Server<Entity>, world: WorldMut) {
        server
            .entity_mut(world, &self.entity)
            .insert_component(self.component);
    }
}

//// Remove Component ////

pub(crate) struct RemoveComponent<R: Replicate> {
    entity: Entity,
    phantom_r: PhantomData<R>,
}

impl<R: Replicate> RemoveComponent<R> {
    pub fn new(entity: &Entity) -> Self {
        RemoveComponent {
            entity: *entity,
            phantom_r: PhantomData,
        }
    }
}

impl<R: Replicate> Command for RemoveComponent<R> {
    fn write(self: Box<Self>, server: &mut Server<Entity>, world: WorldMut) {
        server
            .entity_mut(world, &self.entity)
            .remove_component::<R>();
    }
}
