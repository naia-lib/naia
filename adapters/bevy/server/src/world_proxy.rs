use std::any::TypeId;

use bevy_ecs::{
    entity::Entity,
    world::{Mut, World},
};

use naia_bevy_shared::{ComponentFieldUpdate, ComponentKind, ComponentUpdate, EntityAndGlobalEntityConverter, LocalEntityAndGlobalEntityConverter, Replicate, SerdeErr, WorldMutType, WorldRefType, ReplicaDynMutWrapper, GlobalWorldManagerType, ReplicaDynRefWrapper, ReplicaMutWrapper, ReplicaRefWrapper, ReplicatedComponent, WorldData, ComponentMut, ComponentRef, WorldEntities, ComponentAccess, GlobalEntity, EntityDoesNotExistError};

use crate::{world_entity::WorldId, world_entity::WorldEntity};

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
    main_world: &'w World,
}

impl<'w> WorldRef<'w> {
    pub fn new(main_world: &'w World) -> Self {
        Self { main_world }
    }
}

impl<'w> WorldRefType<WorldEntity> for WorldRef<'w> {
    fn has_entity(&self, world_entity: &WorldEntity) -> bool {
        let world = get_world_from_id(self.main_world, world_entity);
        has_entity(world, &world_entity.entity())
    }

    fn entities(&self) -> Vec<WorldEntity> {
        todo!() // unsure if this is needed for the Bevy implementation?
    }

    fn has_component<R: ReplicatedComponent>(&self, world_entity: &WorldEntity) -> bool {
        let world = get_world_from_id(self.main_world, world_entity);
        has_component::<R>(world, &world_entity.entity())
    }

    fn has_component_of_kind(&self, world_entity: &WorldEntity, component_kind: &ComponentKind) -> bool {
        let world = get_world_from_id(self.main_world, world_entity);
        has_component_of_kind(world, &world_entity.entity(), component_kind)
    }

    fn component<R: ReplicatedComponent>(&self, world_entity: &WorldEntity) -> Option<ReplicaRefWrapper<R>> {
        let world = get_world_from_id(self.main_world, world_entity);
        component(world, &world_entity.entity())
    }

    fn component_of_kind(
        &self,
        world_entity: &WorldEntity,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynRefWrapper> {
        let world = get_world_from_id(self.main_world, world_entity);
        component_of_kind(world, &world_entity.entity(), component_kind)
    }
}

// WorldMut

pub struct WorldMut<'w> {
    main_world: &'w mut World,
}

impl<'w> WorldMut<'w> {
    pub fn new(world: &'w mut World) -> Self {
        Self { main_world: world }
    }
}

impl<'w> WorldRefType<WorldEntity> for WorldMut<'w> {
    fn has_entity(&self, world_entity: &WorldEntity) -> bool {
        let world = get_world_from_id(self.main_world, world_entity);
        has_entity(world, &world_entity.entity())
    }

    fn entities(&self) -> Vec<WorldEntity> {
        todo!(); // unsure if this is needed for the Bevy implementation?
    }

    fn has_component<R: ReplicatedComponent>(&self, world_entity: &WorldEntity) -> bool {
        let world = get_world_from_id(self.main_world, world_entity);
        has_component::<R>(world, &world_entity.entity())
    }

    fn has_component_of_kind(&self, world_entity: &WorldEntity, component_kind: &ComponentKind) -> bool {
        let world = get_world_from_id(self.main_world, world_entity);
        has_component_of_kind(world, &world_entity.entity(), component_kind)
    }

    fn component<R: ReplicatedComponent>(&self, world_entity: &WorldEntity) -> Option<ReplicaRefWrapper<R>> {
        let world = get_world_from_id(self.main_world, world_entity);
        component(world, &world_entity.entity())
    }

    fn component_of_kind(
        &self,
        world_entity: &WorldEntity,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynRefWrapper> {
        let world = get_world_from_id(self.main_world, world_entity);
        component_of_kind(world, &world_entity.entity(), component_kind)
    }
}

impl<'w> WorldMutType<WorldEntity> for WorldMut<'w> {
    fn spawn_entity(&mut self) -> WorldEntity {
        let entity = self.main_world.spawn_empty().id();

        let mut world_entities = world_entities_unchecked_mut(self.main_world);
        world_entities.spawn_entity(&entity);

        WorldEntity::main_new(entity)
    }

    fn local_duplicate_entity(&mut self, entity: &WorldEntity) -> WorldEntity {
        let new_entity = WorldMutType::<WorldEntity>::spawn_entity(self);

        WorldMutType::<WorldEntity>::local_duplicate_components(self, &new_entity, entity);

        new_entity
    }

    fn local_duplicate_components(&mut self, mutable_entity: &WorldEntity, immutable_entity: &WorldEntity) {
        for component_kind in WorldMutType::<WorldEntity>::component_kinds(self, immutable_entity) {
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

    fn despawn_entity(&mut self, world_entity: &WorldEntity) {

        let entity = world_entity.entity();

        get_world_mut_from_id(self.main_world, world_entity, |world| {
            let mut world_entities = world_entities_unchecked_mut(world);
            world_entities.despawn_entity(&entity);

            world.despawn(entity);
        });
    }

    fn component_kinds(&mut self, world_entity: &WorldEntity) -> Vec<ComponentKind> {
        let mut kinds = Vec::new();

        let entity = world_entity.entity();
        let world_data = world_data(&self.main_world);
        let world = get_world_from_id(self.main_world, world_entity);

        let components = world.components();

        for component_id in world.entity(entity).archetype().components() {
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
        world_entity: &WorldEntity,
    ) -> Option<ReplicaMutWrapper<R>> {

        let world_id = world_entity.world_id();
        let entity = world_entity.entity();

        if world_id.is_main() {
            if let Some(bevy_mut) = self.main_world.get_mut::<R>(entity) {
                let wrapper = ComponentMut(bevy_mut);
                let component_mut = ReplicaMutWrapper::new(wrapper);
                return Some(component_mut);
            }
            None
        } else {
            panic!("component_mut() does not yet work for sub-worlds");
        }
    }

    fn component_mut_of_kind(
        &mut self,
        world_entity: &WorldEntity,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynMutWrapper> {

        let world_id = world_entity.world_id();
        let entity = world_entity.entity();

        if world_id.is_main() {
            let accessor = get_accessor(self.main_world, component_kind);
            accessor.component_mut(self.main_world, &entity)
        } else {
            panic!("component_mut_of_kind() does not yet work for sub-worlds");
        }
    }

    fn component_apply_update(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        world_entity: &WorldEntity,
        component_kind: &ComponentKind,
        update: ComponentUpdate,
    ) -> Result<(), SerdeErr> {
        let entity = world_entity.entity();

        let accessor = get_accessor(self.main_world, component_kind);

        get_world_mut_from_id(self.main_world, world_entity, |world| {
            let Some(mut component) = accessor.component_mut(world, &entity) else {
                panic!("ComponentKind has not been registered?");
            };
            component.read_apply_update(converter, update)
        })
    }

    fn component_apply_field_update(
        &mut self,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        world_entity: &WorldEntity,
        component_kind: &ComponentKind,
        update: ComponentFieldUpdate,
    ) -> Result<(), SerdeErr> {

        let entity = world_entity.entity();

        let accessor = get_accessor(self.main_world, component_kind);

        get_world_mut_from_id(self.main_world, world_entity, |world| {
            let Some(mut component) = accessor.component_mut(world, &entity) else {
                panic!("ComponentKind has not been registered?");
            };
            component.read_apply_field_update(converter, update)
        })
    }

    fn mirror_entities(&mut self, new_entity: &WorldEntity, old_entity: &WorldEntity) {
        for component_kind in WorldMutType::<WorldEntity>::component_kinds(self, old_entity) {
            WorldMutType::<WorldEntity>::mirror_components(
                self,
                new_entity,
                old_entity,
                &component_kind,
            );
        }
    }

    fn mirror_components(
        &mut self,
        mutable_world_entity: &WorldEntity,
        immutable_world_entity: &WorldEntity,
        component_kind: &ComponentKind,
    ) {
        if mutable_world_entity.world_id() != immutable_world_entity.world_id() {
            panic!("Entities must be in the same world!");
        }
        let mutable_entity = mutable_world_entity.entity();
        let immutable_entity = immutable_world_entity.entity();

        let accessor = get_accessor(self.main_world, component_kind);

        get_world_mut_from_id(self.main_world, mutable_world_entity, |world| {
            accessor.mirror_components(world, &mutable_entity, &immutable_entity);
        })
    }

    fn insert_component<R: ReplicatedComponent>(&mut self, world_entity: &WorldEntity, component_ref: R) {
        let entity = world_entity.entity();
        get_world_mut_from_id(self.main_world, world_entity, |world| {
            world.entity_mut(entity).insert(component_ref);
        });
    }

    fn insert_boxed_component(&mut self, world_entity: &WorldEntity, boxed_component: Box<dyn Replicate>) {
        let entity = world_entity.entity();
        let component_kind = boxed_component.kind();

        let accessor = get_accessor(self.main_world, &component_kind);

        get_world_mut_from_id(self.main_world, world_entity, |world| {
            accessor.insert_component(world, &entity, boxed_component);
        });
    }

    fn remove_component<R: ReplicatedComponent>(&mut self, world_entity: &WorldEntity) -> Option<R> {
        let entity = world_entity.entity();
        get_world_mut_from_id(self.main_world, world_entity, |world| {
            world.entity_mut(entity).take::<R>()
        })
    }

    fn remove_component_of_kind(
        &mut self,
        world_entity: &WorldEntity,
        component_kind: &ComponentKind,
    ) -> Option<Box<dyn Replicate>> {

        let entity = world_entity.entity();

        let accessor = get_accessor(self.main_world, component_kind);

        get_world_mut_from_id(self.main_world, world_entity, |world| {
            accessor.remove_component(world, &entity)
        })
    }

    fn entity_publish(
        &mut self,
        converter: &dyn EntityAndGlobalEntityConverter<WorldEntity>,
        global_world_manager: &dyn GlobalWorldManagerType,
        world_entity: &WorldEntity,
    ) {
        for component_kind in WorldMutType::<WorldEntity>::component_kinds(self, world_entity) {
            WorldMutType::<WorldEntity>::component_publish(
                self,
                converter,
                global_world_manager,
                world_entity,
                &component_kind,
            );
        }
    }

    fn component_publish(
        &mut self,
        converter: &dyn EntityAndGlobalEntityConverter<WorldEntity>,
        global_world_manager: &dyn GlobalWorldManagerType,
        world_entity: &WorldEntity,
        component_kind: &ComponentKind,
    ) {
        let world_id = world_entity.world_id();
        let entity = world_entity.entity();

        let accessor = get_accessor(self.main_world, component_kind);

        get_world_mut_from_id(self.main_world, world_entity, |world| {
            let converter = WorldSpecificConverter::new(world_id, converter);
            accessor.component_publish(&converter, global_world_manager, world, &entity);
        });
    }

    fn entity_unpublish(&mut self, world_entity: &WorldEntity) {
        for component_kind in WorldMutType::<WorldEntity>::component_kinds(self, world_entity) {
            WorldMutType::<WorldEntity>::component_unpublish(self, world_entity, &component_kind);
        }
    }

    fn component_unpublish(&mut self, world_entity: &WorldEntity, component_kind: &ComponentKind) {
        let entity = world_entity.entity();

        let accessor = get_accessor(self.main_world, component_kind);

        get_world_mut_from_id(self.main_world, world_entity, |world| {
            accessor.component_unpublish(world, &entity);
        });
    }

    fn entity_enable_delegation(
        &mut self,
        converter: &dyn EntityAndGlobalEntityConverter<WorldEntity>,
        global_world_manager: &dyn GlobalWorldManagerType,
        entity: &WorldEntity,
    ) {
        for component_kind in WorldMutType::<WorldEntity>::component_kinds(self, entity) {
            WorldMutType::<WorldEntity>::component_enable_delegation(
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
        converter: &dyn EntityAndGlobalEntityConverter<WorldEntity>,
        global_world_manager: &dyn GlobalWorldManagerType,
        world_entity: &WorldEntity,
        component_kind: &ComponentKind,
    ) {
        let world_id = world_entity.world_id();
        let entity = world_entity.entity();

        let accessor = get_accessor(self.main_world, component_kind);

        get_world_mut_from_id(self.main_world, world_entity, |world| {
            let converter = WorldSpecificConverter::new(world_id, converter);
            accessor.component_enable_delegation(&converter, global_world_manager, world, &entity);
        });
    }

    fn entity_disable_delegation(&mut self, entity: &WorldEntity) {
        for component_kind in WorldMutType::<WorldEntity>::component_kinds(self, entity) {
            WorldMutType::<WorldEntity>::component_disable_delegation(self, entity, &component_kind);
        }
    }

    fn component_disable_delegation(&mut self, world_entity: &WorldEntity, component_kind: &ComponentKind) {

        let entity = world_entity.entity();

        let accessor = get_accessor(self.main_world, component_kind);

        get_world_mut_from_id(self.main_world, world_entity, |world| {
            accessor.component_disable_delegation(world, &entity);
        });
    }
}

struct WorldSpecificConverter<'a> {
    world_id: WorldId,
    inner: &'a dyn EntityAndGlobalEntityConverter<WorldEntity>,
}

impl<'a> WorldSpecificConverter<'a> {
    fn new(world_id: WorldId, inner: &'a dyn EntityAndGlobalEntityConverter<WorldEntity>) -> Self {
        Self { world_id, inner }
    }
}

impl<'a> EntityAndGlobalEntityConverter<Entity> for WorldSpecificConverter<'a> {
    fn global_entity_to_entity(&self, global_entity: &GlobalEntity) -> Result<Entity, EntityDoesNotExistError> {
        let world_entity = self.inner.global_entity_to_entity(global_entity)?;
        Ok(world_entity.entity())
    }

    fn entity_to_global_entity(&self, entity: &Entity) -> Result<GlobalEntity, EntityDoesNotExistError> {
        let world_entity = WorldEntity::new(self.world_id, *entity);
        self.inner.entity_to_global_entity(&world_entity)
    }
}

fn get_accessor(main_world: &World, component_kind: &ComponentKind) -> Box<dyn ComponentAccess> {
    let world_data = world_data(main_world);
    let Some(accessor) = world_data.component_access(component_kind) else {
        panic!("ComponentKind has not been registered?");
    };
    let accessor = accessor.box_clone();
    accessor
}

// private static methods

fn has_entity(world: &World, entity: &Entity) -> bool {
    world.get_entity(*entity).is_ok()
}

// fn entities(world: &World) -> Vec<Entity> {
//     let world_entities = world_entities(world);
//     world_entities.entities()
// }

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

// fn world_entities(world: &World) -> &WorldEntities {
//     world
//         .get_resource::<WorldEntities>()
//         .expect("Need to instantiate by adding WorldEntities resource at startup!")
// }

fn world_entities_unchecked_mut(world: &mut World) -> Mut<WorldEntities> {
    unsafe {
        world
            .as_unsafe_world_cell()
            .get_resource_mut::<WorldEntities>()
            .expect("Need to instantiate by adding WorldEntities resource at startup!")
    }
}

fn get_world_from_id<'a, 'b>(main_world: &'a World, world_entity: &'b WorldEntity) -> &'a World {
    let world_id = world_entity.world_id();
    if world_id.is_main() {
        return main_world;
    } else {
        todo!()
        //return sub_worlds.get_world(&world_id);
    }
}

pub(crate) fn get_world_mut_from_id<U>(
    main_world: &mut World,
    world_entity: &WorldEntity,
    f: impl FnOnce(&mut World) -> U
) -> U {
    let world_id = world_entity.world_id();
    if world_id.is_main() {
        return f(main_world);
    } else {
        todo!()
        // return f(sub_world);
    }
}