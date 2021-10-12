use std::{any::Any, marker::PhantomData, ops::Deref};

use hecs::World;

use naia_shared::{ImplRef, ProtocolType};

use super::entity::Entity;

// ComponentAccess
pub trait ComponentAccess<P: ProtocolType> {
    fn get_component(&self, world: &World, entity: &Entity) -> Option<P>;
    fn remove_component(&self, world: &mut World, entity: &Entity) -> Option<P>;
}

// ComponentAccessor
pub struct ComponentAccessor<P: ProtocolType, R: ImplRef<P>> {
    phantom_p: PhantomData<P>,
    phantom_r: PhantomData<R>,
}

impl<P: ProtocolType, R: ImplRef<P>> ComponentAccessor<P, R> {
    pub fn new() -> Box<dyn Any> {
        let inner_box: Box<dyn ComponentAccess<P>> = Box::new(ComponentAccessor {
            phantom_p: PhantomData::<P>,
            phantom_r: PhantomData::<R>,
        });
        return Box::new(inner_box);
    }
}

impl<P: ProtocolType, R: ImplRef<P>> ComponentAccess<P> for ComponentAccessor<P, R> {
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

fn get_component_ref<P: ProtocolType, R: ImplRef<P>>(world: &World, entity: &Entity) -> Option<R> {
    return world
        .get::<R>(**entity)
        .map_or(None, |v| Some(v.deref().clone_ref()));
}

fn remove_component_ref<P: ProtocolType, R: ImplRef<P>>(world: &mut World, entity: &Entity) -> Option<R> {
    return world
        .remove_one::<R>(**entity)
        .map_or(None, |v| Some(v.clone_ref()));
}
