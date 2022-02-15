use std::marker::PhantomData;

use bevy::ecs::entity::Entity;

use naia_server::{
    shared::{Protocolize, Replicate},
    Server, UserKey,
};

use naia_bevy_shared::WorldMut;

// Command Trait

pub trait Command<P: Protocolize>: Send + Sync + 'static {
    fn write(self: Box<Self>, server: &mut Server<P, Entity>, world: WorldMut);
}

//// Despawn Component ////

#[derive(Debug)]
pub(crate) struct DespawnEntity {
    entity: Entity,
}

impl DespawnEntity {
    pub fn new(entity: &Entity) -> Self {
        return DespawnEntity { entity: *entity };
    }
}

impl<P: Protocolize> Command<P> for DespawnEntity {
    fn write(self: Box<Self>, server: &mut Server<P, Entity>, world: WorldMut) {
        server.entity_mut(world, &self.entity).despawn();
    }
}

//// Insert Component ////

#[derive(Debug)]
pub(crate) struct InsertComponent<P: Protocolize, R: Replicate<P>> {
    entity: Entity,
    component: R,
    phantom_p: PhantomData<P>,
}

impl<P: Protocolize, R: Replicate<P>> InsertComponent<P, R> {
    pub fn new(entity: &Entity, component: R) -> Self {
        return InsertComponent {
            entity: *entity,
            component,
            phantom_p: PhantomData,
        };
    }
}

impl<P: Protocolize, R: Replicate<P>> Command<P> for InsertComponent<P, R> {
    fn write(self: Box<Self>, server: &mut Server<P, Entity>, world: WorldMut) {
        server
            .entity_mut(world, &self.entity)
            .insert_component(self.component);
    }
}

//// Remove Component ////

#[derive(Debug)]
pub(crate) struct RemoveComponent<P: Protocolize, R: Replicate<P>> {
    entity: Entity,
    phantom_p: PhantomData<P>,
    phantom_r: PhantomData<R>,
}

impl<P: Protocolize, R: Replicate<P>> RemoveComponent<P, R> {
    pub fn new(entity: &Entity) -> Self {
        return RemoveComponent {
            entity: *entity,
            phantom_p: PhantomData,
            phantom_r: PhantomData,
        };
    }
}

impl<P: Protocolize, R: Replicate<P>> Command<P> for RemoveComponent<P, R> {
    fn write(self: Box<Self>, server: &mut Server<P, Entity>, world: WorldMut) {
        server
            .entity_mut(world, &self.entity)
            .remove_component::<R>();
    }
}
