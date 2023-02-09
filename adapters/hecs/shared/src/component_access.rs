use std::{any::Any, marker::PhantomData};

use hecs::{Entity, World};

use naia_shared::{ReplicaDynMutWrapper, ReplicaDynRefWrapper, Replicate};

use super::component_ref::{ComponentDynMut, ComponentDynRef};

// ComponentAccess
pub trait ComponentAccess {
    fn component<'w>(&self, world: &'w World, entity: &Entity) -> Option<ReplicaDynRefWrapper<'w>>;
    fn component_mut<'w>(
        &self,
        world: &'w mut World,
        entity: &Entity,
    ) -> Option<ReplicaDynMutWrapper<'w>>;
    fn remove_component(&self, world: &mut World, entity: &Entity) -> Option<Box<dyn Replicate>>;
    fn mirror_components(
        &self,
        world: &mut World,
        mutable_entity: &Entity,
        immutable_entity: &Entity,
    );
}

// ComponentAccessor
pub struct ComponentAccessor<R: Replicate> {
    phantom_r: PhantomData<R>,
}

impl<R: Replicate> ComponentAccessor<R> {
    pub fn create() -> Box<dyn Any> {
        let inner_box: Box<dyn ComponentAccess> = Box::new(ComponentAccessor {
            phantom_r: PhantomData::<R>,
        });
        Box::new(inner_box)
    }
}

impl<R: Replicate> ComponentAccess for ComponentAccessor<R> {
    fn component<'w>(&self, world: &'w World, entity: &Entity) -> Option<ReplicaDynRefWrapper<'w>> {
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
    ) -> Option<ReplicaDynMutWrapper<'w>> {
        if let Ok(hecs_mut) = world.get::<&mut R>(*entity) {
            let wrapper = ComponentDynMut(hecs_mut);
            let component_dyn_mut = ReplicaDynMutWrapper::new(wrapper);
            return Some(component_dyn_mut);
        }
        None
    }

    fn remove_component(&self, world: &mut World, entity: &Entity) -> Option<Box<dyn Replicate>> {
        world
            .remove_one::<R>(*entity)
            .map_or(None, |v| Some(Box::new(v)))
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
                    mutable_component.mirror(immutable_component);
                }
            }
        }
    }
}
