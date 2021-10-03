use std::{any::TypeId, collections::HashMap, marker::PhantomData, ops::Deref};

use hecs::{World as HecsWorld};

use naia_server::{ImplRef, EntityType, ProtocolType, Ref, Replicate, WorldType};

use super::entity::Entity;

// ComponentAccess
pub trait ComponentAccess<P: ProtocolType> {
    fn get_component(&self, world: &World<P>, entity_key: &Entity) -> Option<P>;
}

// ComponentAccessor
pub struct ComponentAccessor<P: ProtocolType, R: ImplRef<P>> {
    phantom_p: PhantomData<P>,
    phantom_r: PhantomData<R>,
}

impl<P: ProtocolType, R: ImplRef<P>> ComponentAccessor<P, R> {
    pub fn new() -> Box<dyn ComponentAccess<P>> {
        Box::new(ComponentAccessor {
            phantom_p: PhantomData::<P>,
            phantom_r: PhantomData::<R>,
        })
    }
}

impl<P: ProtocolType, R: ImplRef<P>> ComponentAccess<P> for ComponentAccessor<P, R> {
    fn get_component(&self, world: &World<P>, entity_key: &Entity) -> Option<P> {
        if let Ok(component_ref) = world.hecs.get::<R>(*entity_key) {
            return Some(component_ref.deref().protocol());
        }
        return None;
    }
}