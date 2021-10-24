use std::{any::Any, marker::PhantomData, ops::Deref};

use bevy::ecs::world::World;

use naia_shared::{ProtocolType, ReplicateSafe, ReplicaDynRefWrapper, ReplicaDynMutWrapper};

use super::{component_ref::{ComponentDynRef, ComponentDynMut}, entity::Entity};

pub trait ComponentAccess<P: ProtocolType>: Send + Sync {
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

pub struct ComponentAccessor<P: ProtocolType, R: ReplicateSafe<P>> {
    phantom_p: PhantomData<P>,
    phantom_r: PhantomData<R>,
}

impl<P: 'static + ProtocolType, R: ReplicateSafe<P>> ComponentAccessor<P, R> {
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
        if let Some(component_ref) = world.get::<R>(**entity) {
            let wrapper = ComponentDynRef(component_ref);
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
        if let Some(component_mut) = world.get_mut::<R>(**entity) {
            let wrapper = ComponentDynMut(component_mut);
            let component_dyn_mut = ReplicaDynMutWrapper::new(wrapper);
            return Some(component_dyn_mut);
        }
        return None;
    }

    fn remove_component(&self, world: &mut World, entity: &Entity) -> Option<P> {
        return world
            .entity_mut(**entity)
            .remove::<R>()
            .map_or(None, |v| Some(v.into_protocol()));
    }
}