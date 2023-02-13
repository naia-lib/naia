use bevy_ecs::{
    entity::Entity,
    world::{Mut, World},
};
use std::any::TypeId;

use naia_shared::{
    ComponentKind, ComponentUpdate, NetEntityHandleConverter, ReplicaDynRefWrapper,
    ReplicaMutWrapper, ReplicaRefWrapper, Replicate, SerdeErr, WorldMutType, WorldRefType,
};

use super::{
    component_ref::{ComponentMut, ComponentRef},
    world_data::WorldData,
};

// WorldProxy

pub trait WorldProxy<'w> {
    fn proxy(self) -> WorldRef<'w>;
}

impl<'w> WorldProxy<'w> for &'w World {
    fn proxy(self) -> WorldRef<'w> {
        WorldRef::new(self)
    }
}

// WorldProxyMut

pub trait WorldProxyMut<'w> {
    fn proxy_mut(self) -> WorldMut<'w>;
}

impl<'w> WorldProxyMut<'w> for &'w mut World {
    fn proxy_mut(self) -> WorldMut<'w> {
        WorldMut::new(self)
    }
}

// WorldRef //

pub struct WorldRef<'w> {
    world: &'w World,
}

impl<'w> WorldRef<'w> {
    pub fn new(world: &'w World) -> Self {
        WorldRef { world }
    }
}

impl<'w> WorldRefType<Entity> for WorldRef<'w> {
    fn has_entity(&self, entity: &Entity) -> bool {
        has_entity(self.world, entity)
    }

    fn entities(&self) -> Vec<Entity> {
        entities(self.world)
    }

    fn has_component<R: Replicate>(&self, entity: &Entity) -> bool {
        has_component::<R>(self.world, entity)
    }

    fn has_component_of_kind(&self, entity: &Entity, component_kind: &ComponentKind) -> bool {
        has_component_of_kind(self.world, entity, component_kind)
    }

    fn component<R: Replicate>(&self, entity: &Entity) -> Option<ReplicaRefWrapper<R>> {
        component(self.world, entity)
    }

    fn component_of_kind(
        &self,
        entity: &Entity,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynRefWrapper> {
        component_of_kind(self.world, entity, component_kind)
    }
}

// WorldMut

pub struct WorldMut<'w> {
    world: &'w mut World,
}

impl<'w> WorldMut<'w> {
    pub fn new(world: &'w mut World) -> Self {
        WorldMut { world }
    }
}

impl<'w> WorldRefType<Entity> for WorldMut<'w> {
    fn has_entity(&self, entity: &Entity) -> bool {
        has_entity(self.world, entity)
    }

    fn entities(&self) -> Vec<Entity> {
        entities(self.world)
    }

    fn has_component<R: Replicate>(&self, entity: &Entity) -> bool {
        has_component::<R>(self.world, entity)
    }

    fn has_component_of_kind(&self, entity: &Entity, component_kind: &ComponentKind) -> bool {
        has_component_of_kind(self.world, entity, component_kind)
    }

    fn component<R: Replicate>(&self, entity: &Entity) -> Option<ReplicaRefWrapper<R>> {
        component(self.world, entity)
    }

    fn component_of_kind(
        &self,
        entity: &Entity,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynRefWrapper> {
        component_of_kind(self.world, entity, component_kind)
    }
}

impl<'w> WorldMutType<Entity> for WorldMut<'w> {
    fn spawn_entity(&mut self) -> Entity {
        let entity = self.world.spawn_empty().id();

        let mut world_data = world_data_unchecked_mut(self.world);
        world_data.spawn_entity(&entity);

        entity
    }

    fn duplicate_entity(&mut self, entity: &Entity) -> Entity {
        let new_entity = WorldMutType::<Entity>::spawn_entity(self);

        WorldMutType::<Entity>::duplicate_components(self, &new_entity, entity);

        new_entity
    }

    fn duplicate_components(&mut self, mutable_entity: &Entity, immutable_entity: &Entity) {
        for component_kind in WorldMutType::<Entity>::component_kinds(self, immutable_entity) {
            let mut component_copy_opt: Option<Box<dyn Replicate>> = None;
            if let Some(component) = self.component_of_kind(immutable_entity, &component_kind) {
                component_copy_opt = Some(component.copy_to_box());
            }
            if let Some(component_copy) = component_copy_opt {
                self.insert_boxed_component(mutable_entity, component_copy);
            }
        }
    }

    fn despawn_entity(&mut self, entity: &Entity) {
        let mut world_data = world_data_unchecked_mut(self.world);
        world_data.despawn_entity(entity);

        self.world.despawn(*entity);
    }

    fn component_kinds(&mut self, entity: &Entity) -> Vec<ComponentKind> {
        let mut kinds = Vec::new();

        let components = self.world.components();

        for component_kind in self.world.entity(*entity).archetype().components() {
            let component_info = components
                .get_info(component_kind)
                .expect("Components need info to instantiate");
            let ref_type = component_info
                .type_id()
                .expect("Components need type_id to instantiate");
            let component_kind = ComponentKind::from(ref_type);
            kinds.push(component_kind);
        }

        kinds
    }

    fn component_mut<R: Replicate>(&mut self, entity: &Entity) -> Option<ReplicaMutWrapper<R>> {
        if let Some(bevy_mut) = self.world.get_mut::<R>(*entity) {
            let wrapper = ComponentMut(bevy_mut);
            let component_mut = ReplicaMutWrapper::new(wrapper);
            return Some(component_mut);
        }
        None
    }

    fn component_apply_update(
        &mut self,
        converter: &dyn NetEntityHandleConverter,
        entity: &Entity,
        component_kind: &ComponentKind,
        update: ComponentUpdate,
    ) -> Result<(), SerdeErr> {
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData>| {
                if let Some(accessor) = data.component_access(component_kind) {
                    if let Some(mut component) = accessor.component_mut(world, entity) {
                        let _update_result = component.read_apply_update(converter, update);
                    }
                }
            });
        Ok(())
    }

    fn mirror_entities(&mut self, new_entity: &Entity, old_entity: &Entity) {
        for component_kind in WorldMutType::<Entity>::component_kinds(self, old_entity) {
            WorldMutType::<Entity>::mirror_components(
                self,
                new_entity,
                old_entity,
                &component_kind,
            );
        }
    }

    fn mirror_components(
        &mut self,
        mutable_entity: &Entity,
        immutable_entity: &Entity,
        component_kind: &ComponentKind,
    ) {
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData>| {
                if let Some(accessor) = data.component_access(component_kind) {
                    accessor.mirror_components(world, mutable_entity, immutable_entity);
                }
            });
    }

    fn insert_component<R: Replicate>(&mut self, entity: &Entity, component_ref: R) {
        // insert into ecs
        self.world.entity_mut(*entity).insert(component_ref);
    }

    fn insert_boxed_component(&mut self, entity: &Entity, boxed_component: Box<dyn Replicate>) {
        let component_kind = boxed_component.kind();
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData>| {
                if let Some(accessor) = data.component_access(&component_kind) {
                    accessor.insert_component(world, entity, boxed_component);
                }
            });
    }

    fn remove_component<R: Replicate>(&mut self, entity: &Entity) -> Option<R> {
        return self.world.entity_mut(*entity).remove::<R>();
    }

    fn remove_component_of_kind(
        &mut self,
        entity: &Entity,
        component_kind: &ComponentKind,
    ) -> Option<Box<dyn Replicate>> {
        let mut output: Option<Box<dyn Replicate>> = None;
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData>| {
                if let Some(accessor) = data.component_access(component_kind) {
                    output = accessor.remove_component(world, entity);
                }
            });
        output
    }
}

// private static methods

fn has_entity(world: &World, entity: &Entity) -> bool {
    return world.get_entity(*entity).is_some();
}

fn entities(world: &World) -> Vec<Entity> {
    let world_data = world_data(world);
    world_data.entities()
}

fn has_component<R: Replicate>(world: &World, entity: &Entity) -> bool {
    return world.get::<R>(*entity).is_some();
}

fn has_component_of_kind(world: &World, entity: &Entity, component_kind: &ComponentKind) -> bool {
    return world
        .entity(*entity)
        .contains_type_id(<ComponentKind as Into<TypeId>>::into(*component_kind));
}

fn component<'a, R: Replicate>(
    world: &'a World,
    entity: &Entity,
) -> Option<ReplicaRefWrapper<'a, R>> {
    if let Some(bevy_ref) = world.get::<R>(*entity) {
        let wrapper = ComponentRef(bevy_ref);
        let component_ref = ReplicaRefWrapper::new(wrapper);
        return Some(component_ref);
    }
    None
}

fn component_of_kind<'a>(
    world: &'a World,
    entity: &Entity,
    component_kind: &ComponentKind,
) -> Option<ReplicaDynRefWrapper<'a>> {
    let world_data = world_data(world);
    if let Some(component_access) = world_data.component_access(component_kind) {
        return component_access.component(world, entity);
    }
    None
}

fn world_data(world: &World) -> &WorldData {
    return world
        .get_resource::<WorldData>()
        .expect("Need to instantiate by adding WorldData<Protocol> resource at startup!");
}

fn world_data_unchecked_mut(world: &World) -> Mut<WorldData> {
    unsafe {
        return world
            .get_resource_unchecked_mut::<WorldData>()
            .expect("Need to instantiate by adding WorldData<Protocol> resource at startup!");
    }
}
