use std::marker::PhantomData;

use bevy::{ecs::world::World, log::debug};

use naia_server::{ProtocolType, ImplRef, Replicate, Ref};

use crate::world::entity::Entity;

// Command Trait

pub trait Command: Send + Sync + 'static {
    fn write(self: Box<Self>, world: &mut World);
}

//// Despawn Component ////

#[derive(Debug)]
pub(crate) struct DespawnEntity {
    entity: Entity,
}

impl DespawnEntity {
    pub fn new(entity: Entity) -> Self {
        return DespawnEntity {
            entity
        };
    }
}

impl Command for DespawnEntity {
    fn write(self: Box<Self>, world: &mut World) {
        if !world.despawn(*self.entity) {
            debug!("Failed to despawn non-existent entity {:?}", self.entity);
        }
    }
}

//// Insert Component ////

#[derive(Debug)]
pub(crate) struct InsertComponent<P: ProtocolType, R: ImplRef<P>> {
    entity: Entity,
    component: R,
    phantom_p: PhantomData<P>,
}

impl<P: ProtocolType, R: ImplRef<P>> InsertComponent<P, R> {
    pub fn new(entity: Entity, component: R) -> Self {
        return InsertComponent {
            entity,
            component,
            phantom_p: PhantomData,
        };
    }
}

impl<P: ProtocolType, R: ImplRef<P>> Command for InsertComponent<P, R>
{
    fn write(self: Box<Self>, world: &mut World) {
        world.entity_mut(*self.entity).insert(self.component);
    }
}

//// Remove Component ////

#[derive(Debug)]
pub(crate) struct RemoveComponent<P: ProtocolType, R: Replicate<P>> {
    entity: Entity,
    phantom_p: PhantomData<P>,
    phantom_r: PhantomData<R>,
}

impl<P: ProtocolType, R: Replicate<P>> RemoveComponent<P, R> {
    pub fn new(entity: Entity) -> Self {
        return RemoveComponent {
            entity,
            phantom_p: PhantomData,
            phantom_r: PhantomData,
        };
    }
}

impl<P: ProtocolType, R: Replicate<P>> Command for RemoveComponent<P, R>
{
    fn write(self: Box<Self>, world: &mut World) {
        if let Some(mut entity_mut) = world.get_entity_mut(*self.entity) {
            entity_mut.remove::<Ref<R>>();
        }
    }
}