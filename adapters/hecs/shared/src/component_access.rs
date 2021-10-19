use std::{any::Any, marker::PhantomData, ops::{Deref, DerefMut}};

use hecs::World;

use naia_shared::{ProtocolType, Replicate};

use super::entity::Entity;

// ComponentAccess
pub trait ComponentAccess<P: ProtocolType> {
    fn get_component(&self, world: &World, entity: &Entity) -> Option<&P>;
    fn get_component_mut(&self, world: &mut World, entity: &Entity) -> Option<&mut P>;
    fn remove_component(&self, world: &mut World, entity: &Entity) -> Option<P>;
}

// ComponentAccessor
pub struct ComponentAccessor<P: ProtocolType, R: Replicate<P>> {
    phantom_p: PhantomData<P>,
    phantom_r: PhantomData<R>,
}

impl<P: ProtocolType, R: Replicate<P>> ComponentAccessor<P, R> {
    pub fn new() -> Box<dyn Any> {
        let inner_box: Box<dyn ComponentAccess<P>> = Box::new(ComponentAccessor {
            phantom_p: PhantomData::<P>,
            phantom_r: PhantomData::<R>,
        });
        return Box::new(inner_box);
    }
}

impl<P: ProtocolType, R: Replicate<P>> ComponentAccess<P> for ComponentAccessor<P, R> {
    fn get_component(&self, world: &World, entity: &Entity) -> Option<&P> {
        return world
            .get::<R>(**entity)
            .map_or(None, |v| Some(&v.deref().to_protocol()));
    }

    fn get_component_mut(&self, world: &mut World, entity: &Entity) -> Option<&mut P> {
        return world
            .get_mut::<R>(**entity)
            .map_or(None, |v| Some(&mut v.deref_mut().to_protocol()));
    }

    fn remove_component(&self, world: &mut World, entity: &Entity) -> Option<P> {
        return world
            .remove_one::<R>(**entity)
            .map_or(None, |v| Some(v.to_protocol()));
    }
}