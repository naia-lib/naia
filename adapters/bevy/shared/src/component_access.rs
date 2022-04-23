use std::{any::Any, marker::PhantomData};

use bevy_ecs::{entity::Entity, world::World};

use naia_shared::{Protocolize, ReplicaDynMutWrapper, ReplicaDynRefWrapper, ReplicateSafe};

use super::component_ref::{ComponentDynMut, ComponentDynRef};

pub trait ComponentAccess<P: Protocolize>: Send + Sync {
    fn component<'w>(
        &self,
        world: &'w World,
        entity: &Entity,
    ) -> Option<ReplicaDynRefWrapper<'w, P>>;
    fn component_mut<'w>(
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

pub struct ComponentAccessor<P: Protocolize, R: ReplicateSafe<P>> {
    phantom_p: PhantomData<P>,
    phantom_r: PhantomData<R>,
}

impl<P: 'static + Protocolize, R: ReplicateSafe<P>> ComponentAccessor<P, R> {
    pub fn create() -> Box<dyn Any> {
        let inner_box: Box<dyn ComponentAccess<P>> = Box::new(ComponentAccessor {
            phantom_p: PhantomData::<P>,
            phantom_r: PhantomData::<R>,
        });
        Box::new(inner_box)
    }
}

impl<P: Protocolize, R: ReplicateSafe<P>> ComponentAccess<P> for ComponentAccessor<P, R> {
    fn component<'w>(
        &self,
        world: &'w World,
        entity: &Entity,
    ) -> Option<ReplicaDynRefWrapper<'w, P>> {
        if let Some(component_ref) = world.get::<R>(*entity) {
            let wrapper = ComponentDynRef(component_ref);
            let component_dyn_ref = ReplicaDynRefWrapper::new(wrapper);
            return Some(component_dyn_ref);
        }
        None
    }

    fn component_mut<'w>(
        &self,
        world: &'w mut World,
        entity: &Entity,
    ) -> Option<ReplicaDynMutWrapper<'w, P>> {
        if let Some(component_mut) = world.get_mut::<R>(*entity) {
            let wrapper = ComponentDynMut(component_mut);
            let component_dyn_mut = ReplicaDynMutWrapper::new(wrapper);
            return Some(component_dyn_mut);
        }
        None
    }

    fn remove_component(&self, world: &mut World, entity: &Entity) -> Option<P> {
        return world
            .entity_mut(*entity)
            .remove::<R>()
            .map(|v| v.into_protocol());
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
