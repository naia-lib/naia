use hecs::{Entity, World};

use naia_shared::{
    ComponentKind, ComponentUpdate, NetEntityHandleConverter, ReplicaDynMutWrapper,
    ReplicaDynRefWrapper, ReplicaMutWrapper, ReplicaRefWrapper, Replicate, SerdeErr, WorldMutType,
    WorldRefType,
};

use super::{
    component_ref::{ComponentMut, ComponentRef},
    world_data::WorldData,
};

// WorldProxy

pub trait WorldProxy<'w, 'd> {
    fn proxy(self, data: &'d WorldData) -> WorldRef<'w, 'd>;
}

impl<'w, 'd> WorldProxy<'w, 'd> for &'w World {
    fn proxy(self, data: &'d WorldData) -> WorldRef<'w, 'd> {
        WorldRef::new(self, data)
    }
}

// WorldProxyMut

pub trait WorldProxyMut<'w, 'd> {
    fn proxy_mut(self, data: &'d mut WorldData) -> WorldMut<'w, 'd>;
}

impl<'w, 'd> WorldProxyMut<'w, 'd> for &'w mut World {
    fn proxy_mut(self, data: &'d mut WorldData) -> WorldMut<'w, 'd> {
        WorldMut::new(self, data)
    }
}

// WorldRef

pub struct WorldRef<'w, 'd> {
    world: &'w World,
    world_data: &'d WorldData,
}

impl<'w, 'd> WorldRef<'w, 'd> {
    pub fn new(world: &'w World, data: &'d WorldData) -> Self {
        WorldRef {
            world,
            world_data: data,
        }
    }
}

impl<'w, 'd> WorldRefType<Entity> for WorldRef<'w, 'd> {
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
        has_component_of_kind(self.world, self.world_data, entity, component_kind)
    }

    fn component<R: Replicate>(&self, entity: &Entity) -> Option<ReplicaRefWrapper<R>> {
        component::<R>(self.world, entity)
    }

    fn component_of_kind<'a>(
        &'a self,
        entity: &Entity,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynRefWrapper<'a>> {
        component_of_kind(self.world, self.world_data, entity, component_kind)
    }
}

// WorldMut

pub struct WorldMut<'w, 'd> {
    world: &'w mut World,
    world_data: &'d mut WorldData,
}

impl<'w, 'd> WorldMut<'w, 'd> {
    pub fn new(world: &'w mut World, data: &'d mut WorldData) -> Self {
        WorldMut {
            world,
            world_data: data,
        }
    }
}

impl<'w, 'd> WorldRefType<Entity> for WorldMut<'w, 'd> {
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
        has_component_of_kind(self.world, self.world_data, entity, component_kind)
    }

    fn component<R: Replicate>(&self, entity: &Entity) -> Option<ReplicaRefWrapper<R>> {
        component::<R>(self.world, entity)
    }

    fn component_of_kind<'a>(
        &'a self,
        entity: &Entity,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynRefWrapper<'a>> {
        component_of_kind(self.world, self.world_data, entity, component_kind)
    }
}

impl<'w, 'd> WorldMutType<Entity> for WorldMut<'w, 'd> {
    fn spawn_entity(&mut self) -> Entity {
        self.world.spawn(())
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
        self.world
            .despawn(*entity)
            .expect("error despawning Entity");
    }

    fn component_kinds(&mut self, entity: &Entity) -> Vec<ComponentKind> {
        let mut kinds = Vec::new();

        if let Ok(entity_ref) = self.world.entity(*entity) {
            for component_type in entity_ref.component_types() {
                kinds.push(ComponentKind::from(component_type));
            }
        }

        kinds
    }

    fn component_mut<R: Replicate>(&mut self, entity: &Entity) -> Option<ReplicaMutWrapper<R>> {
        if let Ok(hecs_mut) = self.world.get::<&mut R>(*entity) {
            let wrapper = ComponentMut(hecs_mut);
            let component_mut = ReplicaMutWrapper::new(wrapper);
            return Some(component_mut);
        }
        None
    }

    fn component_mut_of_kind<'a>(
        &'a mut self,
        entity: &Entity,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynMutWrapper<'a>> {
        if let Some(access) = self.world_data.component_access(component_kind) {
            if let Some(component) = access.component_mut(self.world, entity) {
                return Some(component);
            }
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
        if let Some(access) = self.world_data.component_access(component_kind) {
            if let Some(mut component) = access.component_mut(self.world, entity) {
                component.read_apply_update(converter, update)?;
            }
        }
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
        if let Some(accessor) = self.world_data.component_access(component_kind) {
            accessor.mirror_components(self.world, mutable_entity, immutable_entity);
        }
    }

    fn insert_component<R: Replicate>(&mut self, entity: &Entity, component_ref: R) {
        self.world
            .insert_one(*entity, component_ref)
            .expect("error inserting Component");
    }

    fn insert_boxed_component(&mut self, entity: &Entity, boxed_component: Box<dyn Replicate>) {
        let component_kind = boxed_component.kind();
        if let Some(accessor) = self.world_data.component_access(&component_kind) {
            return accessor.insert_component(&mut self.world, entity, boxed_component);
        } else {
            panic!("shouldn't happen")
        }
    }

    fn remove_component<R: Replicate>(&mut self, entity: &Entity) -> Option<R> {
        self.world.remove_one::<R>(*entity).ok()
    }

    fn remove_component_of_kind(
        &mut self,
        entity: &Entity,
        component_kind: &ComponentKind,
    ) -> Option<Box<dyn Replicate>> {
        if let Some(accessor) = self.world_data.component_access(component_kind) {
            return accessor.remove_component(self.world, entity);
        }
        None
    }
}

// private static methods
fn has_entity(world: &World, entity: &Entity) -> bool {
    world.contains(*entity)
}

fn entities(world: &World) -> Vec<Entity> {
    let mut output = Vec::new();

    for entity in world.iter() {
        output.push(entity.entity());
    }

    output
}

fn has_component<R: Replicate>(world: &World, entity: &Entity) -> bool {
    let result = world.get::<&R>(*entity);
    result.is_ok()
}

fn has_component_of_kind(
    world: &World,
    world_data: &WorldData,
    entity: &Entity,
    component_kind: &ComponentKind,
) -> bool {
    return component_of_kind(world, world_data, entity, component_kind).is_some();
}

fn component<'a, R: Replicate>(
    world: &'a World,
    entity: &Entity,
) -> Option<ReplicaRefWrapper<'a, R>> {
    if let Ok(hecs_ref) = world.get::<&R>(*entity) {
        let wrapper = ComponentRef(hecs_ref);
        let component_ref = ReplicaRefWrapper::new(wrapper);
        return Some(component_ref);
    }
    None
}

fn component_of_kind<'a>(
    world: &'a World,
    world_data: &'a WorldData,
    entity: &Entity,
    component_kind: &ComponentKind,
) -> Option<ReplicaDynRefWrapper<'a>> {
    if let Some(access) = world_data.component_access(component_kind) {
        return access.component(world, entity);
    }
    None
}
