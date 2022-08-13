use std::{any::Any, marker::PhantomData};

use hecs::{Entity, World};

use naia_shared::{Protocolize, ReplicaDynMutWrapper, ReplicaDynRefWrapper, ReplicateSafe};

use super::component_ref::{ComponentDynMut, ComponentDynRef};

// ComponentAccess
pub trait ComponentAccess<P: Protocolize> {
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

// ComponentAccessor
pub struct ComponentAccessor<P: Protocolize, R: ReplicateSafe<P>> {
    phantom_p: PhantomData<P>,
    phantom_r: PhantomData<R>,
}

impl<P: Protocolize, R: ReplicateSafe<P>> ComponentAccessor<P, R> {
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
        if let Ok(hecs_ref) = world.get::<&R>(*entity) {
            let wrapper = ComponentDynRef(hecs_ref);
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
        if let Ok(hecs_mut) = world.get::<&mut R>(*entity) {
            let wrapper = ComponentDynMut(hecs_mut);
            let component_dyn_mut = ReplicaDynMutWrapper::new(wrapper);
            return Some(component_dyn_mut);
        }
        None
    }

    fn remove_component(&self, world: &mut World, entity: &Entity) -> Option<P> {
        world
            .remove_one::<R>(*entity)
            .map_or(None, |v| Some(v.into_protocol()))
    }

    fn mirror_components(
        &self,
        world: &mut World,
        mutable_entity: &Entity,
        immutable_entity: &Entity,
    ) {
        unsafe {
            if let Ok(immutable_component) = world.get_unchecked::<&R>(*immutable_entity) {
                if let Ok(mutable_component) = world.get_unchecked::<&mut R>(*mutable_entity) {
                    mutable_component.mirror(&immutable_component.protocol_copy());
                }
            }
        }
    }
}
