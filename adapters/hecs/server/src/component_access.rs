use std::{any::Any, marker::PhantomData};

use naia_server::{ImplRef, ProtocolType};

use super::{entity::Entity, world_adapt::WorldAdapter};

// ComponentAccess
pub trait ComponentAccess<P: ProtocolType> {
    fn get_component(&self, world: &WorldAdapter, entity_key: &Entity) -> Option<P>;
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
    fn get_component(&self, world: &WorldAdapter, entity: &Entity) -> Option<P> {
        if let Some(component_ref) = world.get_component_ref::<P, R>(entity) {
            return Some(component_ref.protocol());
        }
        return None;
    }
}
