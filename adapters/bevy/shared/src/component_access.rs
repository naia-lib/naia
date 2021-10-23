use std::{marker::PhantomData, ops::Deref};

use bevy::ecs::world::World;

use naia_shared::{ProtocolType, ReplicateSafe};

use super::entity::Entity;

pub trait ComponentAccess<P: ProtocolType>: Send + Sync {
    fn get_component(&self, world: &World, entity: &Entity) -> Option<P>;
    fn remove_component(&self, world: &mut World, entity: &Entity) -> Option<P>;
}

pub struct ComponentAccessor<P: ProtocolType, R: ReplicateSafe<P>> {
    phantom_p: PhantomData<P>,
    phantom_r: PhantomData<R>,
}

impl<P: 'static + ProtocolType, R: ReplicateSafe<P>> ComponentAccessor<P, R> {
    pub fn new() -> Box<dyn ComponentAccess<P>> {
        Box::new(ComponentAccessor {
            phantom_p: PhantomData::<P>,
            phantom_r: PhantomData::<R>,
        })
    }
}

impl<P: ProtocolType, R: ReplicateSafe<P>> ComponentAccess<P> for ComponentAccessor<P, R> {
    fn get_component(&self, world: &World, entity: &Entity) -> Option<P> {
        if let Some(component_ref) = get_component_ref::<P, R>(world, entity) {
            return Some(component_ref.protocol());
        }
        return None;
    }

    fn remove_component(&self, world: &mut World, entity: &Entity) -> Option<P> {
        if let Some(component_ref) = remove_component_ref::<P, R>(world, entity) {
            return Some(component_ref.protocol());
        }
        return None;
    }
}

fn get_component_ref<P: ProtocolType, R: ReplicateSafe<P>>(
    world: &World,
    entity: &Entity,
) -> Option<R> {
    return world
        .get::<R>(**entity)
        .map_or(None, |v| Some(v.deref().clone_ref()));
}

fn remove_component_ref<P: ProtocolType, R: ReplicateSafe<P>>(
    world: &mut World,
    entity: &Entity,
) -> Option<R> {
    return world
        .entity_mut(**entity)
        .remove::<R>()
        .map_or(None, |v| Some(v.clone_ref()));
}
