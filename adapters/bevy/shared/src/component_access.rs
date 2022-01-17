use std::{any::Any, marker::PhantomData};

use bevy::{
    ecs::{entity::Entity, world::World},
    prelude::Component,
};

use naia_shared::{ProtocolType, ReplicaDynMutWrapper, ReplicaDynRefWrapper, ReplicateSafe};

use super::component_ref::{ComponentDynMut, ComponentDynRef};

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
    fn mirror_components(
        &self,
        world: &mut World,
        mutable_entity: &Entity,
        immutable_entity: &Entity,
    );
}

#[derive(Component)]
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
        if let Some(component_ref) = world.get::<R>(*entity) {
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
        if let Some(component_mut) = world.get_mut::<R>(*entity) {
            let wrapper = ComponentDynMut(component_mut);
            let component_dyn_mut = ReplicaDynMutWrapper::new(wrapper);
            return Some(component_dyn_mut);
        }
        return None;
    }

    fn remove_component(&self, world: &mut World, entity: &Entity) -> Option<P> {
        return world
            .entity_mut(*entity)
            .remove::<R>()
            .map_or(None, |v| Some(v.into_protocol()));
    }

    fn mirror_components(
        &self,
        world: &mut World,
        mutable_entity: &Entity,
        immutable_entity: &Entity,
    ) {
        let mut query = world.query::<&mut R>();
        unsafe {
            if let Ok(immutable_component) = query.get_unchecked(world, *immutable_entity) {
                if let Ok(mut mutable_component) = query.get_unchecked(world, *mutable_entity) {
                    mutable_component.mirror(&immutable_component.protocol_copy());
                }
            }
        }
    }
}
