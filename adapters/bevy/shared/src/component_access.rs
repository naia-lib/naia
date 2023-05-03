use std::{any::Any, marker::PhantomData};

use bevy_app::App;
use bevy_ecs::{entity::Entity, schedule::IntoSystemConfigs, world::World};

use naia_shared::{GlobalWorldManagerType, ReplicaDynMutWrapper, ReplicaDynRefWrapper, Replicate};

use super::{
    change_detection::{on_component_added, on_component_removed},
    component_ref::{ComponentDynMut, ComponentDynRef},
    system_set::HostSyncChangeTracking,
};

pub trait ComponentAccess: Send + Sync {
    fn add_systems(&self, app: &mut App);
    fn box_clone(&self) -> Box<dyn ComponentAccess>;
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
    fn component_publish(
        &self,
        global_world_manager: &dyn GlobalWorldManagerType<Entity>,
        world: &mut World,
        entity: &Entity,
    );
    fn component_unpublish(&self, world: &mut World, entity: &Entity);
    fn component_enable_delegation(
        &self,
        global_manager: &dyn GlobalWorldManagerType<Entity>,
        world: &mut World,
        entity: &Entity,
    );
    fn component_disable_delegation(&self, world: &mut World, entity: &Entity);
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
    fn add_systems(&self, app: &mut App) {
        app.add_systems(
            (on_component_added::<R>, on_component_removed::<R>)
                .chain()
                .in_set(HostSyncChangeTracking),
        );
    }

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

    fn box_clone(&self) -> Box<dyn ComponentAccess> {
        let new_me = ComponentAccessor::<R> {
            phantom_r: PhantomData,
        };
        Box::new(new_me)
    }

    fn component_publish(
        &self,
        global_manager: &dyn GlobalWorldManagerType<Entity>,
        world: &mut World,
        entity: &Entity,
    ) {
        if let Some(mut component_mut) = world.get_mut::<R>(*entity) {
            let component_kind = component_mut.kind();
            let diff_mask_size = component_mut.diff_mask_size();
            let mutator =
                global_manager.register_component(entity, &component_kind, diff_mask_size);
            component_mut.publish(&mutator);
        }
    }

    fn component_unpublish(&self, world: &mut World, entity: &Entity) {
        if let Some(mut component_mut) = world.get_mut::<R>(*entity) {
            component_mut.unpublish();
        }
    }

    fn component_enable_delegation(
        &self,
        global_manager: &dyn GlobalWorldManagerType<Entity>,
        world: &mut World,
        entity: &Entity,
    ) {
        if let Some(mut component_mut) = world.get_mut::<R>(*entity) {
            let accessor = global_manager.get_entity_auth_accessor(entity);
            let mutator_opt = {
                if global_manager.entity_is_server_owned_and_remote(entity) {
                    let component_kind = component_mut.kind();
                    let diff_mask_size = component_mut.diff_mask_size();
                    let mutator =
                        global_manager.register_component(entity, &component_kind, diff_mask_size);
                    Some(mutator)
                } else {
                    None
                }
            };
            component_mut.enable_delegation(&accessor, &mutator_opt);
        }
    }

    fn component_disable_delegation(&self, world: &mut World, entity: &Entity) {
        if let Some(mut component_mut) = world.get_mut::<R>(*entity) {
            component_mut.disable_delegation();
        }
    }
}
