use std::any::TypeId;

use bevy_ecs::{
    entity::Entity,
    world::{Mut, World},
};

use naia_bevy_shared::{ComponentMut, ComponentRef, WorldData, ComponentFieldUpdate, ComponentKind, ComponentUpdate, EntityAndGlobalEntityConverter, GlobalWorldManagerType, LocalEntityAndGlobalEntityConverter, ReplicaDynMutWrapper, ReplicaDynRefWrapper, ReplicaMutWrapper, ReplicaRefWrapper, Replicate, ReplicatedComponent, SerdeErr, WorldMutType, WorldRefType, WorldEntities};

// WorldProxyMut

pub trait WorldProxyMut<'w> {
    fn proxy_mut(self) -> WorldMut<'w>;
}

impl<'w> WorldProxyMut<'w> for &'w mut World {
    fn proxy_mut(self) -> WorldMut<'w> {
        WorldMut::new(self)
    }
}

// // WorldRef //
//
// pub struct WorldRef<'w> {
//     world: &'w World,
// }
//
// impl<'w> WorldRef<'w> {
//     pub fn new(world: &'w World) -> Self {
//         WorldRef { world }
//     }
// }
//
// impl<'w> WorldRefType<Entity> for WorldRef<'w> {
//     fn has_entity(&self, entity: &Entity) -> bool {
//         has_entity(self.world, entity)
//     }
//
//     fn entities(&self) -> Vec<Entity> {
//         entities(self.world)
//     }
//
//     fn has_component<R: ReplicatedComponent>(&self, entity: &Entity) -> bool {
//         has_component::<R>(self.world, entity)
//     }
//
//     fn has_component_of_kind(&self, entity: &Entity, component_kind: &ComponentKind) -> bool {
//         has_component_of_kind(self.world, entity, component_kind)
//     }
//
//     fn component<R: ReplicatedComponent>(&self, entity: &Entity) -> Option<ReplicaRefWrapper<R>> {
//         component(self.world, entity)
//     }
//
//     fn component_of_kind(
//         &self,
//         entity: &Entity,
//         component_kind: &ComponentKind,
//     ) -> Option<ReplicaDynRefWrapper> {
//         component_of_kind(self.world, entity, component_kind)
//     }
// }

// WorldMut

pub struct WorldMut<'w> {
    world: &'w mut World,
}

impl<'w> WorldMut<'w> {
    pub fn new(world: &'w mut World) -> Self {
        Self { world }
    }
}

impl<'w> WorldRefType<Entity> for WorldMut<'w> {
    fn has_entity(&self, entity: &Entity) -> bool {
        has_entity(self.world, entity)
    }

    fn entities(&self) -> Vec<Entity> {
        entities(self.world)
    }

    fn has_component<R: ReplicatedComponent>(&self, entity: &Entity) -> bool {
        has_component::<R>(self.world, entity)
    }

    fn has_component_of_kind(&self, entity: &Entity, component_kind: &ComponentKind) -> bool {
        has_component_of_kind(self.world, entity, component_kind)
    }

    fn component<R: ReplicatedComponent>(&self, entity: &Entity) -> Option<ReplicaRefWrapper<R>> {
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

        let mut world_entities = world_entities_unchecked_mut(self.world);
        world_entities.spawn_entity(&entity);

        entity
    }

    fn local_duplicate_entity(&mut self, entity: &Entity) -> Entity {
        let new_entity = WorldMutType::<Entity>::spawn_entity(self);

        WorldMutType::<Entity>::local_duplicate_components(self, &new_entity, entity);

        new_entity
    }

    fn local_duplicate_components(&mut self, mutable_entity: &Entity, immutable_entity: &Entity) {
        for component_kind in WorldMutType::<Entity>::component_kinds(self, immutable_entity) {
            let mut component_copy_opt: Option<Box<dyn Replicate>> = None;
            if let Some(component) = self.component_of_kind(immutable_entity, &component_kind) {
                component_copy_opt = Some(component.copy_to_box());
            }
            if let Some(mut component_copy) = component_copy_opt {
                component_copy.localize();
                self.insert_boxed_component(mutable_entity, component_copy);
            }
        }
    }

    fn despawn_entity(&mut self, entity: &Entity) {
        let mut world_entities = world_entities_unchecked_mut(self.world);
        world_entities.despawn_entity(entity);

        self.world.despawn(*entity);
    }

    fn component_kinds(&mut self, entity: &Entity) -> Vec<ComponentKind> {
        let mut kinds = Vec::new();

        let world_data = world_data(&self.world);

        let components = self.world.components();

        for component_id in self.world.entity(*entity).archetype().components() {
            let component_info = components
                .get_info(component_id)
                .expect("Components need info to instantiate");
            let type_id = component_info
                .type_id()
                .expect("Components need type_id to instantiate");
            let component_kind = ComponentKind::from(type_id);

            if world_data.has_kind(&component_kind) {
                kinds.push(component_kind);
            }
        }

        kinds
    }

    fn component_mut<R: ReplicatedComponent>(
        &mut self,
        entity: &Entity,
    ) -> Option<ReplicaMutWrapper<R>> {
        if let Some(bevy_mut) = self.world.get_mut::<R>(*entity) {
            let wrapper = ComponentMut(bevy_mut);
            let component_mut = ReplicaMutWrapper::new(wrapper);
            return Some(component_mut);
        }
        None
    }

    fn component_mut_of_kind(
        &mut self,
        entity: &Entity,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynMutWrapper> {
        let world_data = world_data(&self.world);
        let Some(component_access) = world_data.component_access(component_kind) else {
            return None;
        };
        let new_component_access = component_access.box_clone();
        new_component_access.component_mut(self.world, entity)
    }

    fn component_apply_update(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        entity: &Entity,
        component_kind: &ComponentKind,
        update: ComponentUpdate,
    ) -> Result<(), SerdeErr> {
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData>| {
                let Some(accessor) = data.component_access(component_kind) else {
                    panic!("ComponentKind has not been registered?");
                };
                if let Some(mut component) = accessor.component_mut(world, entity) {
                    let _update_result = component.read_apply_update(converter, update);
                }
            });
        Ok(())
    }

    fn component_apply_field_update(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        entity: &Entity,
        component_kind: &ComponentKind,
        update: ComponentFieldUpdate,
    ) -> Result<(), SerdeErr> {
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData>| {
                let Some(accessor) = data.component_access(component_kind) else {
                    panic!("ComponentKind has not been registered?");
                };
                if let Some(mut component) = accessor.component_mut(world, entity) {
                    let _update_result = component.read_apply_field_update(converter, update);
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
                let Some(accessor) = data.component_access(component_kind) else {
                    panic!("ComponentKind has not been registered?");
                };
                accessor.mirror_components(world, mutable_entity, immutable_entity);
            });
    }

    fn insert_component<R: ReplicatedComponent>(&mut self, entity: &Entity, component_ref: R) {
        // insert into ecs
        self.world.entity_mut(*entity).insert(component_ref);
    }

    fn insert_boxed_component(&mut self, entity: &Entity, boxed_component: Box<dyn Replicate>) {
        let component_kind = boxed_component.kind();
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData>| {
                let Some(accessor) = data.component_access(&component_kind) else {
                    panic!("ComponentKind has not been registered?");
                };
                accessor.insert_component(world, entity, boxed_component);
            });
    }

    fn remove_component<R: ReplicatedComponent>(&mut self, entity: &Entity) -> Option<R> {
        self.world.entity_mut(*entity).take::<R>()
    }

    fn remove_component_of_kind(
        &mut self,
        entity: &Entity,
        component_kind: &ComponentKind,
    ) -> Option<Box<dyn Replicate>> {
        let mut output: Option<Box<dyn Replicate>> = None;
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData>| {
                let Some(accessor) = data.component_access(component_kind) else {
                    panic!("ComponentKind has not been registered?");
                };
                output = accessor.remove_component(world, entity);
            });
        output
    }

    fn entity_publish(
        &mut self,
        converter: &dyn EntityAndGlobalEntityConverter<Entity>,
        global_world_manager: &dyn GlobalWorldManagerType,
        entity: &Entity,
    ) {
        for component_kind in WorldMutType::<Entity>::component_kinds(self, entity) {
            WorldMutType::<Entity>::component_publish(
                self,
                converter,
                global_world_manager,
                entity,
                &component_kind,
            );
        }
    }

    fn component_publish(
        &mut self,
        converter: &dyn EntityAndGlobalEntityConverter<Entity>,
        global_world_manager: &dyn GlobalWorldManagerType,
        world_entity: &Entity,
        component_kind: &ComponentKind,
    ) {
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData>| {
                let Some(accessor) = data.component_access(component_kind) else {
                    panic!("ComponentKind has not been registered?");
                };
                accessor.component_publish(converter, global_world_manager, world, world_entity);
            });
    }

    fn entity_unpublish(&mut self, world_entity: &Entity) {
        for component_kind in WorldMutType::<Entity>::component_kinds(self, world_entity) {
            WorldMutType::<Entity>::component_unpublish(self, world_entity, &component_kind);
        }
    }

    fn component_unpublish(&mut self, entity: &Entity, component_kind: &ComponentKind) {
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData>| {
                let Some(accessor) = data.component_access(component_kind) else {
                    panic!("ComponentKind has not been registered?");
                };
                accessor.component_unpublish(world, entity);
            });
    }

    fn entity_enable_delegation(
        &mut self,
        converter: &dyn EntityAndGlobalEntityConverter<Entity>,
        global_world_manager: &dyn GlobalWorldManagerType,
        entity: &Entity,
    ) {
        for component_kind in WorldMutType::<Entity>::component_kinds(self, entity) {
            WorldMutType::<Entity>::component_enable_delegation(
                self,
                converter,
                global_world_manager,
                entity,
                &component_kind,
            );
        }
    }

    fn component_enable_delegation(
        &mut self,
        converter: &dyn EntityAndGlobalEntityConverter<Entity>,
        global_world_manager: &dyn GlobalWorldManagerType,
        entity: &Entity,
        component_kind: &ComponentKind,
    ) {
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData>| {
                let Some(accessor) = data.component_access(component_kind) else {
                    panic!("ComponentKind has not been registered?");
                };
                accessor.component_enable_delegation(converter, global_world_manager, world, entity);
            });
    }

    fn entity_disable_delegation(&mut self, entity: &Entity) {
        for component_kind in WorldMutType::<Entity>::component_kinds(self, entity) {
            WorldMutType::<Entity>::component_disable_delegation(self, entity, &component_kind);
        }
    }

    fn component_disable_delegation(&mut self, entity: &Entity, component_kind: &ComponentKind) {
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData>| {
                let Some(accessor) = data.component_access(component_kind) else {
                    panic!("ComponentKind has not been registered?");
                };
                accessor.component_disable_delegation(world, entity);
            });
    }
}

// private static methods

fn has_entity(world: &World, entity: &Entity) -> bool {
    world.get_entity(*entity).is_ok()
}

fn entities(world: &World) -> Vec<Entity> {
    let world_entities = world_entities(world);
    world_entities.entities()
}

fn has_component<R: ReplicatedComponent>(world: &World, entity: &Entity) -> bool {
    world.get::<R>(*entity).is_some()
}

fn has_component_of_kind(world: &World, entity: &Entity, component_kind: &ComponentKind) -> bool {
    world
        .entity(*entity)
        .contains_type_id(<ComponentKind as Into<TypeId>>::into(*component_kind))
}

fn component<'a, R: ReplicatedComponent>(
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
    let Some(component_access) = world_data.component_access(component_kind) else {
        panic!("ComponentKind has not been registered?");
    };
    component_access.component(world, entity)
}

fn world_data(world: &World) -> &WorldData {
    world
        .get_resource::<WorldData>()
        .expect("Need to instantiate by adding WorldData<Protocol> resource at startup!")
}

// fn world_data_unchecked_mut(world: &mut World) -> Mut<WorldData> {
//     unsafe {
//         world
//             .as_unsafe_world_cell()
//             .get_resource_mut::<WorldData>()
//             .expect("Need to instantiate by adding WorldData<Protocol> resource at startup!")
//     }
// }

fn world_entities(world: &World) -> &WorldEntities {
    world
        .get_resource::<WorldEntities>()
        .expect("Need to instantiate by adding WorldEntities resource at startup!")
}

fn world_entities_unchecked_mut(world: &mut World) -> Mut<WorldEntities> {
    unsafe {
        world
            .as_unsafe_world_cell()
            .get_resource_mut::<WorldEntities>()
            .expect("Need to instantiate by adding WorldEntities resource at startup!")
    }
}