use std::{any::Any, marker::PhantomData};

use bevy_ecs::{entity::Entity, world::World};

use naia_shared::{ReplicaDynMutWrapper, ReplicaDynRefWrapper, Replicate};

use super::component_ref::{ComponentDynMut, ComponentDynRef};

pub trait ComponentAccess: Send + Sync {
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
    fn insert_component(
        &self,
        world: &mut World,
        entity: &Entity,
        boxed_component: Box<dyn Replicate>,
    );
}

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
    ) -> Option<ReplicaDynMutWrapper<'w>> {
        if let Some(component_mut) = world.get_mut::<R>(*entity) {
            let wrapper = ComponentDynMut(component_mut);
            let component_dyn_mut = ReplicaDynMutWrapper::new(wrapper);
            return Some(component_dyn_mut);
        }
        None
    }

    fn remove_component(&self, world: &mut World, entity: &Entity) -> Option<Box<dyn Replicate>> {
        let result: Option<R> = world.entity_mut(*entity).take::<R>();
        let casted: Option<Box<dyn Replicate>> = result.map(|inner: R| {
            let boxed_r: Box<R> = Box::new(inner);
            let boxed_dyn: Box<dyn Replicate> = boxed_r;
            boxed_dyn
        });
        casted
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
                    let some_r: &R = &immutable_component;
                    mutable_component.mirror(some_r);
                }
            }
        }
    }

    fn insert_component(
        &self,
        world: &mut World,
        entity: &Entity,
        boxed_component: Box<dyn Replicate>,
    ) {
        let boxed_any = boxed_component.to_boxed_any();
        let inner: R = *(boxed_any.downcast::<R>().unwrap());
        world.entity_mut(*entity).insert(inner);
    }
}
