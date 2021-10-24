use std::{any::Any, marker::PhantomData};

use hecs::World;

use naia_shared::{ProtocolType, ReplicaDynMutWrapper, ReplicaDynRefWrapper, ReplicateSafe};

use super::{
    entity::Entity,
    component_ref::{ComponentDynMut, ComponentDynRef},
};

// ComponentAccess
pub trait ComponentAccess<P: ProtocolType> {
    fn get_component<'w>(
        &self,
        world: &'w World,
        entity: &Entity,
    ) -> Option<ReplicaDynRefWrapper<'w, P>>;
    fn get_component_mut<'w>(
        &self,
        world: &'w mut World,
        entity: &Entity,
    ) -> Option<ReplicaDynMutWrapper<'w, P>>;
    fn remove_component(&self, world: &mut World, entity: &Entity) -> Option<P>;
}

// ComponentAccessor
pub struct ComponentAccessor<P: ProtocolType, R: ReplicateSafe<P>> {
    phantom_p: PhantomData<P>,
    phantom_r: PhantomData<R>,
}

impl<P: ProtocolType, R: ReplicateSafe<P>> ComponentAccessor<P, R> {
    pub fn new() -> Box<dyn Any> {
        let inner_box: Box<dyn ComponentAccess<P>> = Box::new(ComponentAccessor {
            phantom_p: PhantomData::<P>,
            phantom_r: PhantomData::<R>,
        });
        return Box::new(inner_box);
    }
}

impl<P: ProtocolType, R: ReplicateSafe<P>> ComponentAccess<P> for ComponentAccessor<P, R> {
    fn get_component<'w>(
        &self,
        world: &'w World,
        entity: &Entity,
    ) -> Option<ReplicaDynRefWrapper<'w, P>> {
        if let Ok(hecs_ref) = world.get::<R>(**entity) {
            let wrapper = ComponentDynRef(hecs_ref);
            let component_dyn_ref = ReplicaDynRefWrapper::new(wrapper);
            return Some(component_dyn_ref);
        }
        return None;
    }

    fn get_component_mut<'w>(
        &self,
        world: &'w mut World,
        entity: &Entity,
    ) -> Option<ReplicaDynMutWrapper<'w, P>> {
        if let Ok(hecs_mut) = world.get_mut::<R>(**entity) {
            let wrapper = ComponentDynMut(hecs_mut);
            let component_dyn_mut = ReplicaDynMutWrapper::new(wrapper);
            return Some(component_dyn_mut);
        }
        return None;
    }

    fn remove_component(&self, world: &mut World, entity: &Entity) -> Option<P> {
        return world
            .remove_one::<R>(**entity)
            .map_or(None, |v| Some(v.into_protocol()));
    }
}
