use std::{any::Any, marker::PhantomData, ops::{Deref, DerefMut}};

use hecs::{World, Ref as HecsRef, RefMut as HecsMut, Component as HecsComponent};

use naia_shared::{ProtocolType, ReplicateSafe, ComponentDynRef, ComponentDynMut, ComponentDynRefTrait, ComponentDynMutTrait};

use super::entity::Entity;

// ComponentAccess
pub trait ComponentAccess<P: ProtocolType> {
    fn get_component<'w>(&self, world: &'w World, entity: &Entity) -> Option<ComponentDynRef<'w, P>>;
    fn get_component_mut<'w>(&self, world: &'w mut World, entity: &Entity) -> Option<ComponentDynMut<'w, P>>;
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
    fn get_component<'w>(&self, world: &'w World, entity: &Entity) -> Option<ComponentDynRef<'w, P>> {
        if let Ok(hecs_ref) = world.get::<R>(**entity) {
            let wrapper = RefWrapper(hecs_ref);
            let component_dyn_ref = ComponentDynRef::new(wrapper);
            return Some(component_dyn_ref);
        }
        return None;
    }

    fn get_component_mut<'w>(&self, world: &'w mut World, entity: &Entity) -> Option<ComponentDynMut<'w, P>> {
        if let Ok(hecs_mut) = world.get_mut::<R>(**entity) {
            let wrapper = MutWrapper(hecs_mut);
            let component_dyn_mut = ComponentDynMut::new(wrapper);
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

////

// ComponentDynRef
struct RefWrapper<'a, T: HecsComponent>(HecsRef<'a, T>);

impl<'a, P: ProtocolType, R: ReplicateSafe<P>> ComponentDynRefTrait<P> for RefWrapper<'a, R> {
    fn component_dyn_deref(&self) -> &dyn ReplicateSafe<P> {
        return self.0.deref();
    }
}

// ComponentDynMut
struct MutWrapper<'a, T: HecsComponent>(HecsMut<'a, T>);

impl<'a, P: ProtocolType, R: ReplicateSafe<P>> ComponentDynRefTrait<P> for MutWrapper<'a, R> {
    fn component_dyn_deref(&self) -> &dyn ReplicateSafe<P> {
        return self.0.deref();
    }
}

impl<'a, P: ProtocolType, R: ReplicateSafe<P>> ComponentDynMutTrait<P> for MutWrapper<'a, R> {
    fn component_dyn_deref_mut(&mut self) -> &mut dyn ReplicateSafe<P> {
        return self.0.deref_mut();
    }
}