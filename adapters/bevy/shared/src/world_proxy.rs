use std::marker::PhantomData;
use bevy::ecs::{
    entity::Entity,
    world::{Mut, World},
};
use bevy::ecs::component::SparseStorage;
use bevy::prelude::Component;

use naia_shared::{
    DiffMask, PacketReader, ProtocolInserter, ProtocolKindType, ProtocolType, ReplicaDynRefWrapper,
    ReplicaMutWrapper, ReplicaRefWrapper, ReplicateSafe, WorldMutType, WorldRefType,
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
        return WorldRef::new(self);
    }
}

// WorldProxyMut

pub trait WorldProxyMut<'w> {
    fn proxy_mut(self) -> WorldMut<'w>;
}

impl<'w> WorldProxyMut<'w> for &'w mut World {
    fn proxy_mut(self) -> WorldMut<'w> {
        return WorldMut::new(self);
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

impl<'w, P: 'static + ProtocolType> WorldRefType<P, Entity> for WorldRef<'w> {
    fn has_entity(&self, entity: &Entity) -> bool {
        return has_entity(self.world, entity);
    }

    fn entities(&self) -> Vec<Entity> {
        return entities::<P>(self.world);
    }

    fn has_component<R: ReplicateSafe<P>>(&self, entity: &Entity) -> bool {
        return has_component::<P, R>(self.world, entity);
    }

    fn has_component_of_kind(&self, entity: &Entity, component_kind: &P::Kind) -> bool {
        return has_component_of_kind::<P>(self.world, entity, component_kind);
    }

    fn get_component<R: ReplicateSafe<P>>(
        &self,
        entity: &Entity,
    ) -> Option<ReplicaRefWrapper<P, R>> {
        return get_component(self.world, entity);
    }

    fn get_component_of_kind(
        &self,
        entity: &Entity,
        component_kind: &P::Kind,
    ) -> Option<ReplicaDynRefWrapper<P>> {
        return get_component_of_kind::<P>(self.world, entity, component_kind);
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

impl<'w, P: 'static + ProtocolType> WorldRefType<P, Entity> for WorldMut<'w> {
    fn has_entity(&self, entity: &Entity) -> bool {
        return has_entity(self.world, entity);
    }

    fn entities(&self) -> Vec<Entity> {
        return entities::<P>(self.world);
    }

    fn has_component<R: ReplicateSafe<P>>(&self, entity: &Entity) -> bool {
        return has_component::<P, R>(self.world, entity);
    }

    fn has_component_of_kind(&self, entity: &Entity, component_kind: &P::Kind) -> bool {
        return has_component_of_kind::<P>(self.world, entity, component_kind);
    }

    fn get_component<R: ReplicateSafe<P>>(
        &self,
        entity: &Entity,
    ) -> Option<ReplicaRefWrapper<P, R>> {
        return get_component(self.world, entity);
    }

    fn get_component_of_kind(
        &self,
        entity: &Entity,
        component_kind: &P::Kind,
    ) -> Option<ReplicaDynRefWrapper<P>> {
        return get_component_of_kind(self.world, entity, component_kind);
    }
}

impl<'w, P: 'static + ProtocolType> WorldMutType<P, Entity> for WorldMut<'w> {
    fn get_component_mut<R: ReplicateSafe<P>>(
        &mut self,
        entity: &Entity,
    ) -> Option<ReplicaMutWrapper<P, R>> {
        if let Some(bevy_mut) = self.world.get_mut::<ReplicateSafeComponent<P, R>>(*entity) {
            let wrapper = ComponentMut(&mut bevy_mut.into_inner().inner);
            let component_mut = ReplicaMutWrapper::new(wrapper);
            return Some(component_mut);
        }
        return None;
    }

    fn component_read_partial(
        &mut self,
        entity: &Entity,
        component_kind: &P::Kind,
        diff_mask: &DiffMask,
        reader: &mut PacketReader,
        packet_index: u16,
    ) {
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData<P>>| {
                if let Some(accessor) = data.get_component_access(component_kind) {
                    if let Some(mut component) = accessor.get_component_mut(world, entity) {
                        component.read_partial(diff_mask, reader, packet_index);
                    }
                }
            });
    }

    fn mirror_components(
        &mut self,
        mutable_entity: &Entity,
        immutable_entity: &Entity,
        component_kind: &P::Kind,
    ) {
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData<P>>| {
                if let Some(accessor) = data.get_component_access(component_kind) {
                    accessor.mirror_components(world, mutable_entity, immutable_entity);
                }
            });
    }

    fn spawn_entity(&mut self) -> Entity {
        let entity = self.world.spawn().id();

        let mut world_data = get_world_data_unchecked_mut::<P>(&mut self.world);
        world_data.spawn_entity(&entity);

        return entity;
    }

    fn despawn_entity(&mut self, entity: &Entity) {
        let mut world_data = get_world_data_unchecked_mut::<P>(&self.world);
        world_data.despawn_entity(entity);

        self.world.despawn(*entity);
    }

    fn get_component_kinds(&mut self, entity: &Entity) -> Vec<P::Kind> {
        let mut kinds = Vec::new();

        let components = self.world.components();

        for component_id in self.world.entity(*entity).archetype().components() {
            let component_info = components
                .get_info(component_id)
                .expect("Components need info to instantiate");
            let ref_type = component_info
                .type_id()
                .expect("Components need type_id to instantiate");
            let kind = P::type_to_kind(ref_type);
            kinds.push(kind);
        }

        return kinds;
    }

    fn insert_component<I: ReplicateSafe<P>>(&mut self, entity: &Entity, component_ref: I) {
        // cache type id for later
        // todo: can we initialize this map on startup via Protocol derive?
        let mut world_data = get_world_data_unchecked_mut(&self.world);
        let component_kind = component_ref.get_kind();
        if !world_data.has_kind(&component_kind) {
            world_data.put_kind::<I>(&component_kind);
        }

        // insert into ecs
        self.world.entity_mut(*entity).insert(ReplicateSafeComponent { inner: component_ref, _proto: Default::default() });
    }

    fn remove_component<R: ReplicateSafe<P>>(&mut self, entity: &Entity) -> Option<R> {
        return self.world.entity_mut(*entity).remove::<ReplicateSafeComponent<P, R>>().map(|f| f.inner);
    }

    fn remove_component_of_kind(&mut self, entity: &Entity, component_kind: &P::Kind) -> Option<P> {
        let mut output: Option<P> = None;
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData<P>>| {
                if let Some(accessor) = data.get_component_access(component_kind) {
                    output = accessor.remove_component(world, entity);
                }
            });
        return output;
    }
}

impl<'w, P: ProtocolType> ProtocolInserter<P, Entity> for WorldMut<'w> {
    fn insert<I: ReplicateSafe<P>>(&mut self, entity: &Entity, impl_ref: I) {
        self.insert_component::<I>(entity, impl_ref);
    }
}

// private static methods

fn has_entity(world: &World, entity: &Entity) -> bool {
    return world.get_entity(*entity).is_some();
}

fn entities<P: ProtocolType>(world: &World) -> Vec<Entity> {
    let world_data = get_world_data::<P>(world);
    return world_data.get_entities();
}

fn has_component<P: ProtocolType, R: ReplicateSafe<P>>(world: &World, entity: &Entity) -> bool {
    return world.get::<ReplicateSafeComponent<P, R>>(*entity).is_some();
}

fn has_component_of_kind<P: ProtocolType>(
    world: &World,
    entity: &Entity,
    component_kind: &P::Kind,
) -> bool {
    return world
        .entity(*entity)
        .contains_type_id(component_kind.to_type_id());
}

fn get_component<'a, P: ProtocolType, R: ReplicateSafe<P>>(
    world: &'a World,
    entity: &Entity,
) -> Option<ReplicaRefWrapper<'a, P, R>> {
    if let Some(bevy_ref) = world.get::<ReplicateSafeComponent<P, R>>(*entity) {
        let wrapper = ComponentRef(&bevy_ref.inner);
        let component_ref = ReplicaRefWrapper::new(wrapper);
        return Some(component_ref);
    }
    return None;
}

fn get_component_of_kind<'a, P: ProtocolType>(
    world: &'a World,
    entity: &Entity,
    component_kind: &P::Kind,
) -> Option<ReplicaDynRefWrapper<'a, P>> {
    let world_data = get_world_data(world);
    if let Some(component_access) = world_data.get_component_access(component_kind) {
        return component_access.get_component(world, entity);
    }
    return None;
}

fn get_world_data<P: ProtocolType>(world: &World) -> &WorldData<P> {
    return world
        .get_resource::<WorldData<P>>()
        .expect("Need to instantiate by adding WorldData<Protocol> resource at startup!");
}

fn get_world_data_unchecked_mut<P: ProtocolType>(world: &World) -> Mut<WorldData<P>> {
    unsafe {
        return world
            .get_resource_unchecked_mut::<WorldData<P>>()
            .expect("Need to instantiate by adding WorldData<Protocol> resource at startup!");
    }
}

pub struct ReplicateSafeComponent<P: ProtocolType, R: ReplicateSafe<P>> {
    pub(crate) inner: R,
    _proto: PhantomData<P>,
}

// FIXME: how should an API consumer decide which storage to use?
impl<P: ProtocolType, R: ReplicateSafe<P>> Component for ReplicateSafeComponent<P, R> { type Storage = SparseStorage; }
