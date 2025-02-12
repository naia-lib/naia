use std::{any::Any, marker::PhantomData};

use bevy_app::{App, Update};
use bevy_ecs::{component::Component, entity::Entity, schedule::IntoSystemConfigs, world::World};

use naia_shared::{EntityAndGlobalEntityConverter, GlobalWorldManagerType, ReplicaDynMutWrapper, ReplicaDynRefWrapper, Replicate};

use super::{
    change_detection::{on_component_added, on_component_removed},
    component_ref::{ComponentDynMut, ComponentDynRef},
    system_set::HostSyncChangeTracking,
};

pub trait AppTag: Send + Sync + 'static {
    fn add_systems(boxed_component: Box<dyn ComponentAccess>, app: &mut App);
}

pub trait ComponentAccess: Send + Sync {
    fn add_systems(&self, app: &mut App);
    fn box_clone(&self) -> Box<dyn ComponentAccess>;
    fn component<'w>(&self, world: &'w World, world_entity: &Entity) -> Option<ReplicaDynRefWrapper<'w>>;
    fn component_mut<'w>(
        &self,
        world: &'w mut World,
        world_entity: &Entity,
    ) -> Option<ReplicaDynMutWrapper<'w>>;
    fn remove_component(&self, world: &mut World, world_entity: &Entity) -> Option<Box<dyn Replicate>>;
    fn mirror_components(
        &self,
        world: &mut World,
        mutable_world_entity: &Entity,
        immutable_world_entity: &Entity,
    );
    fn insert_component(
        &self,
        world: &mut World,
        world_entity: &Entity,
        boxed_component: Box<dyn Replicate>,
    );
    fn component_publish(
        &self,
        converter: &dyn EntityAndGlobalEntityConverter<Entity>,
        global_world_manager: &dyn GlobalWorldManagerType,
        world: &mut World,
        world_entity: &Entity,
    );
    fn component_unpublish(&self, world: &mut World, world_entity: &Entity);
    fn component_enable_delegation(
        &self,
        converter: &dyn EntityAndGlobalEntityConverter<Entity>,
        global_manager: &dyn GlobalWorldManagerType,
        world: &mut World,
        world_entity: &Entity,
    );
    fn component_disable_delegation(&self, world: &mut World, world_entity: &Entity);
}

pub struct ComponentAccessor<R: Replicate> {
    phantom_r: PhantomData<R>,
}

impl<R: Replicate + Component> ComponentAccessor<R> {
    fn new() -> Self {
        Self {
            phantom_r: PhantomData::<R>,
        }
    }

    pub fn create() -> Box<dyn Any> {
        let inner_box: Box<dyn ComponentAccess> = Box::new(ComponentAccessor::<R>::new());
        Box::new(inner_box)
    }
}

impl<R: Replicate + Component> ComponentAccess for ComponentAccessor<R> {
    fn add_systems(&self, app: &mut App) {
        app.add_systems(
            Update,
            (on_component_added::<R>, on_component_removed::<R>)
                .chain()
                .in_set(HostSyncChangeTracking),
        );
    }

    fn component<'w>(&self, world: &'w World, world_entity: &Entity) -> Option<ReplicaDynRefWrapper<'w>> {
        if let Some(component_ref) = world.get::<R>(*world_entity) {
            let wrapper = ComponentDynRef(component_ref);
            let component_dyn_ref = ReplicaDynRefWrapper::new(wrapper);
            return Some(component_dyn_ref);
        }
        None
    }

    fn component_mut<'w>(
        &self,
        world: &'w mut World,
        world_entity: &Entity,
    ) -> Option<ReplicaDynMutWrapper<'w>> {
        if let Some(component_mut) = world.get_mut::<R>(*world_entity) {
            let wrapper = ComponentDynMut(component_mut);
            let component_dyn_mut = ReplicaDynMutWrapper::new(wrapper);
            return Some(component_dyn_mut);
        }
        None
    }

    fn remove_component(&self, world: &mut World, world_entity: &Entity) -> Option<Box<dyn Replicate>> {
        let result: Option<R> = world.entity_mut(*world_entity).take::<R>();
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
        mutable_world_entity: &Entity,
        immutable_world_entity: &Entity,
    ) {
        let mut query = world.query::<&mut R>();
        unsafe {
            let world = world.as_unsafe_world_cell();
            if let Ok(immutable_component) = query.get_unchecked(world, *immutable_world_entity) {
                if let Ok(mut mutable_component) = query.get_unchecked(world, *mutable_world_entity) {
                    let some_r: &R = &immutable_component;
                    mutable_component.mirror(some_r);
                }
            }
        }
    }

    fn insert_component(
        &self,
        world: &mut World,
        world_entity: &Entity,
        boxed_component: Box<dyn Replicate>,
    ) {
        let boxed_any = boxed_component.to_boxed_any();
        let inner: R = *(boxed_any.downcast::<R>().unwrap());
        world.entity_mut(*world_entity).insert(inner);
    }

    fn box_clone(&self) -> Box<dyn ComponentAccess> {
        let new_me = ComponentAccessor::<R> {
            phantom_r: PhantomData,
        };
        Box::new(new_me)
    }

    fn component_publish(
        &self,
        converter: &dyn EntityAndGlobalEntityConverter<Entity>,
        global_manager: &dyn GlobalWorldManagerType,
        world: &mut World,
        world_entity: &Entity,
    ) {
        if let Some(mut component_mut) = world.get_mut::<R>(*world_entity) {
            let component_kind = component_mut.kind();
            let diff_mask_size = component_mut.diff_mask_size();
            let global_entity = converter.entity_to_global_entity(world_entity).unwrap();
            let mutator =
                global_manager.register_component(&global_entity, &component_kind, diff_mask_size);
            component_mut.publish(&mutator);
        }
    }

    fn component_unpublish(&self, world: &mut World, world_entity: &Entity) {
        if let Some(mut component_mut) = world.get_mut::<R>(*world_entity) {
            component_mut.unpublish();
        }
    }

    fn component_enable_delegation(
        &self,
        converter: &dyn EntityAndGlobalEntityConverter<Entity>,
        global_manager: &dyn GlobalWorldManagerType,
        world: &mut World,
        world_entity: &Entity,
    ) {
        if let Some(mut component_mut) = world.get_mut::<R>(*world_entity) {
            let global_entity = converter.entity_to_global_entity(world_entity).unwrap();
            let accessor = global_manager.get_entity_auth_accessor(&global_entity);
            if global_manager.entity_needs_mutator_for_delegation(&global_entity) {
                let component_kind = component_mut.kind();
                let diff_mask_size = component_mut.diff_mask_size();
                let mutator =
                    global_manager.register_component(&global_entity, &component_kind, diff_mask_size);
                component_mut.enable_delegation(&accessor, Some(&mutator));
            } else {
                component_mut.enable_delegation(&accessor, None);
            }
        }
    }

    fn component_disable_delegation(&self, world: &mut World, world_entity: &Entity) {
        if let Some(mut component_mut) = world.get_mut::<R>(*world_entity) {
            component_mut.disable_delegation();
        }
    }
}
