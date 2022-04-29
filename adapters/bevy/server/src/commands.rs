use std::marker::PhantomData;

use bevy_ecs::entity::Entity;

use naia_server::{
    shared::{ChannelIndex, Protocolize, Replicate, ReplicateSafe},
    Server,
};

use naia_bevy_shared::WorldMut;

// Command Trait

pub trait Command<P: Protocolize, C: ChannelIndex>: Send + Sync + 'static {
    fn write(self: Box<Self>, server: &mut Server<P, Entity, C>, world: WorldMut);
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

impl<P: Protocolize, C: ChannelIndex> Command<P, C> for DespawnEntity {
    fn write(self: Box<Self>, server: &mut Server<P, Entity, C>, world: WorldMut) {
        server.entity_mut(world, &self.entity).despawn();
    }
}

//// Insert Component ////

pub(crate) struct InsertComponent<P: Protocolize, R: ReplicateSafe<P>> {
    entity: Entity,
    component: R,
    phantom_p: PhantomData<P>,
}

impl<P: Protocolize, R: ReplicateSafe<P>> InsertComponent<P, R> {
    pub fn new(entity: &Entity, component: R) -> Self {
        InsertComponent {
            entity: *entity,
            component,
            phantom_p: PhantomData,
        }
    }
}

impl<P: Protocolize, R: ReplicateSafe<P>, C: ChannelIndex> Command<P, C> for InsertComponent<P, R> {
    fn write(self: Box<Self>, server: &mut Server<P, Entity, C>, world: WorldMut) {
        server
            .entity_mut(world, &self.entity)
            .insert_component(self.component);
    }
}

//// Remove Component ////

pub(crate) struct RemoveComponent<P: Protocolize, R: Replicate<P>> {
    entity: Entity,
    phantom_p: PhantomData<P>,
    phantom_r: PhantomData<R>,
}

impl<P: Protocolize, R: Replicate<P>> RemoveComponent<P, R> {
    pub fn new(entity: &Entity) -> Self {
        RemoveComponent {
            entity: *entity,
            phantom_p: PhantomData,
            phantom_r: PhantomData,
        }
    }
}

impl<P: Protocolize, R: Replicate<P>, C: ChannelIndex> Command<P, C> for RemoveComponent<P, R> {
    fn write(self: Box<Self>, server: &mut Server<P, Entity, C>, world: WorldMut) {
        server
            .entity_mut(world, &self.entity)
            .remove_component::<R>();
    }
}
