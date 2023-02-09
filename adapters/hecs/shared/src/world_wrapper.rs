use std::{
    default::Default,
    ops::{Deref, DerefMut},
};

use hecs::{Entity, World};

use naia_shared::{
    ComponentKind, ComponentUpdate, NetEntityHandleConverter, ReplicaDynRefWrapper,
    ReplicaMutWrapper, ReplicaRefWrapper, Replicate, SerdeErr, WorldMutType, WorldRefType,
};

use crate::{
    component_ref::{ComponentMut, ComponentRef},
    WorldData,
};

#[derive(Default)]
pub struct WorldWrapper {
    pub inner: World,
    data: WorldData,
}

impl WorldWrapper {
    pub fn wrap(world: World) -> Self {
        Self {
            inner: world,
            data: WorldData::default(),
        }
    }

    pub fn new() -> Self {
        Self {
            inner: World::new(),
            data: WorldData::default(),
        }
    }
}

impl Deref for WorldWrapper {
    type Target = World;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for WorldWrapper {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl WorldRefType<Entity> for &WorldWrapper {
    fn has_entity(&self, entity: &Entity) -> bool {
        has_entity(&self.inner, entity)
    }

    fn entities(&self) -> Vec<Entity> {
        entities(&self.inner)
    }

    fn has_component<R: Replicate>(&self, entity: &Entity) -> bool {
        has_component::<R>(&self.inner, entity)
    }

    fn has_component_of_kind(&self, entity: &Entity, component_kind: &ComponentKind) -> bool {
        has_component_of_kind(&self.inner, &self.data, entity, component_kind)
    }

    fn component<R: Replicate>(&self, entity: &Entity) -> Option<ReplicaRefWrapper<R>> {
        component::<R>(&self.inner, entity)
    }

    fn component_of_kind<'a>(
        &'a self,
        entity: &Entity,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynRefWrapper<'a>> {
        component_of_kind(&self.inner, &self.data, entity, component_kind)
    }
}

impl WorldRefType<Entity> for &mut WorldWrapper {
    fn has_entity(&self, entity: &Entity) -> bool {
        has_entity(&self.inner, entity)
    }

    fn entities(&self) -> Vec<Entity> {
        entities(&self.inner)
    }

    fn has_component<R: Replicate>(&self, entity: &Entity) -> bool {
        has_component::<R>(&self.inner, entity)
    }

    fn has_component_of_kind(&self, entity: &Entity, component_kind: &ComponentKind) -> bool {
        has_component_of_kind(&self.inner, &self.data, entity, component_kind)
    }

    fn component<R: Replicate>(&self, entity: &Entity) -> Option<ReplicaRefWrapper<R>> {
        component::<R>(&self.inner, entity)
    }

    fn component_of_kind<'a>(
        &'a self,
        entity: &Entity,
        component_kind: &ComponentKind,
    ) -> Option<ReplicaDynRefWrapper<'a>> {
        component_of_kind(&self.inner, &self.data, entity, component_kind)
    }
}

impl WorldMutType<Entity> for &mut WorldWrapper {
    fn spawn_entity(&mut self) -> Entity {
        self.inner.spawn(())
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
                //Protocolize::extract_and_insert(&component_copy, mutable_entity, self);
                todo!();
            }
        }
    }

    fn despawn_entity(&mut self, entity: &Entity) {
        self.inner
            .despawn(*entity)
            .expect("error despawning Entity");
    }

    fn component_kinds(&mut self, entity: &Entity) -> Vec<ComponentKind> {
        let mut kinds = Vec::new();

        if let Ok(entity_ref) = self.inner.entity(*entity) {
            for component_type in entity_ref.component_types() {
                kinds.push(ComponentKind::from(component_type));
            }
        }

        kinds
    }

    fn component_mut<R: Replicate>(&mut self, entity: &Entity) -> Option<ReplicaMutWrapper<R>> {
        if let Ok(hecs_mut) = self.inner.get::<&mut R>(*entity) {
            let wrapper = ComponentMut(hecs_mut);
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
        if let Some(access) = self.data.component_access(component_kind) {
            if let Some(mut component) = access.component_mut(&mut self.inner, entity) {
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
        if let Some(accessor) = self.data.component_access(component_kind) {
            accessor.mirror_components(&mut self.inner, mutable_entity, immutable_entity);
        }
    }

    fn insert_component<R: Replicate>(&mut self, entity: &Entity, component_ref: R) {
        // cache type id for later
        // todo: can we initialize this map on startup via Protocol derive?
        let component_kind = component_ref.kind();
        if !self.data.has_kind(&component_kind) {
            self.data.put_kind::<R>(&component_kind);
        }

        self.inner
            .insert_one(*entity, component_ref)
            .expect("error inserting Component");
    }

    fn insert_boxed_component(&mut self, entity: &Entity, boxed_component: Box<dyn Replicate>) {
        todo!()
    }

    fn remove_component<R: Replicate>(&mut self, entity: &Entity) -> Option<R> {
        self.inner.remove_one::<R>(*entity).ok()
    }

    fn remove_component_of_kind(
        &mut self,
        entity: &Entity,
        component_kind: &ComponentKind,
    ) -> Option<Box<dyn Replicate>> {
        if let Some(accessor) = self.data.component_access(component_kind) {
            return accessor.remove_component(&mut self.inner, entity);
        }
        None
    }
}

// impl<P: Protocolize> ProtocolInserter<P, Entity> for &mut WorldWrapper<P> {
//     fn insert<I: Replicate<P>>(&mut self, entity: &Entity, impl_ref: I) {
//         self.insert_component::<I>(entity, impl_ref);
//     }
// }

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
