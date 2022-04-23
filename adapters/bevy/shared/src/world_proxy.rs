use bevy_ecs::{
    entity::Entity,
    world::{Mut, World},
};

use naia_shared::{
    ComponentUpdate, NetEntityHandleConverter, ProtocolInserter, ProtocolKindType, Protocolize,
    ReplicaDynRefWrapper, ReplicaMutWrapper, ReplicaRefWrapper, ReplicateSafe, WorldMutType,
    WorldRefType,
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

impl<'w, P: Protocolize> WorldRefType<P, Entity> for WorldRef<'w> {
    fn has_entity(&self, entity: &Entity) -> bool {
        has_entity(self.world, entity)
    }

    fn entities(&self) -> Vec<Entity> {
        entities::<P>(self.world)
    }

    fn has_component<R: ReplicateSafe<P>>(&self, entity: &Entity) -> bool {
        has_component::<P, R>(self.world, entity)
    }

    fn has_component_of_kind(&self, entity: &Entity, component_kind: &P::Kind) -> bool {
        has_component_of_kind::<P>(self.world, entity, component_kind)
    }

    fn component<R: ReplicateSafe<P>>(&self, entity: &Entity) -> Option<ReplicaRefWrapper<P, R>> {
        component(self.world, entity)
    }

    fn component_of_kind(
        &self,
        entity: &Entity,
        component_kind: &P::Kind,
    ) -> Option<ReplicaDynRefWrapper<P>> {
        component_of_kind::<P>(self.world, entity, component_kind)
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

impl<'w, P: Protocolize> WorldRefType<P, Entity> for WorldMut<'w> {
    fn has_entity(&self, entity: &Entity) -> bool {
        has_entity(self.world, entity)
    }

    fn entities(&self) -> Vec<Entity> {
        entities::<P>(self.world)
    }

    fn has_component<R: ReplicateSafe<P>>(&self, entity: &Entity) -> bool {
        has_component::<P, R>(self.world, entity)
    }

    fn has_component_of_kind(&self, entity: &Entity, component_kind: &P::Kind) -> bool {
        has_component_of_kind::<P>(self.world, entity, component_kind)
    }

    fn component<R: ReplicateSafe<P>>(&self, entity: &Entity) -> Option<ReplicaRefWrapper<P, R>> {
        component(self.world, entity)
    }

    fn component_of_kind(
        &self,
        entity: &Entity,
        component_kind: &P::Kind,
    ) -> Option<ReplicaDynRefWrapper<P>> {
        component_of_kind(self.world, entity, component_kind)
    }
}

impl<'w, P: Protocolize> WorldMutType<P, Entity> for WorldMut<'w> {
    fn spawn_entity(&mut self) -> Entity {
        let entity = self.world.spawn().id();

        let mut world_data = world_data_unchecked_mut::<P>(self.world);
        world_data.spawn_entity(&entity);

        entity
    }

    fn duplicate_entity(&mut self, entity: &Entity) -> Entity {
        let new_entity = WorldMutType::<P, Entity>::spawn_entity(self);

        WorldMutType::<P, Entity>::duplicate_components(self, &new_entity, entity);

        new_entity
    }

    fn duplicate_components(&mut self, mutable_entity: &Entity, immutable_entity: &Entity) {
        for component_kind in WorldMutType::<P, Entity>::component_kinds(self, immutable_entity) {
            let mut component_copy_opt: Option<P> = None;
            if let Some(component) = self.component_of_kind(immutable_entity, &component_kind) {
                component_copy_opt = Some(component.protocol_copy());
            }
            if let Some(component_copy) = component_copy_opt {
                Protocolize::extract_and_insert(&component_copy, mutable_entity, self);
            }
        }
    }

    fn despawn_entity(&mut self, entity: &Entity) {
        let mut world_data = world_data_unchecked_mut::<P>(self.world);
        world_data.despawn_entity(entity);

        self.world.despawn(*entity);
    }

    fn component_kinds(&mut self, entity: &Entity) -> Vec<P::Kind> {
        let mut kinds = Vec::new();

        let components = self.world.components();

        for component_id in self.world.entity(*entity).archetype().components() {
            let component_info = components
                .get_info(component_id)
                .expect("Components need info to instantiate");
            let ref_type = component_info
                .type_id()
                .expect("Components need type_id to instantiate");
            if let Some(kind) = P::type_to_kind(ref_type) {
                kinds.push(kind);
            }
        }

        kinds
    }

    fn component_mut<R: ReplicateSafe<P>>(
        &mut self,
        entity: &Entity,
    ) -> Option<ReplicaMutWrapper<P, R>> {
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
        component_kind: &P::Kind,
        update: ComponentUpdate<P::Kind>,
    ) {
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData<P>>| {
                if let Some(accessor) = data.component_access(component_kind) {
                    if let Some(mut component) = accessor.component_mut(world, entity) {
                        component.read_apply_update(converter, update);
                    }
                }
            });
    }

    fn mirror_entities(&mut self, new_entity: &Entity, old_entity: &Entity) {
        for component_kind in WorldMutType::<P, Entity>::component_kinds(self, old_entity) {
            WorldMutType::<P, Entity>::mirror_components(
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
        component_kind: &P::Kind,
    ) {
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData<P>>| {
                if let Some(accessor) = data.component_access(component_kind) {
                    accessor.mirror_components(world, mutable_entity, immutable_entity);
                }
            });
    }

    fn insert_component<I: ReplicateSafe<P>>(&mut self, entity: &Entity, component_ref: I) {
        // cache type id for later
        // todo: can we initialize this map on startup via Protocol derive?
        let mut world_data = world_data_unchecked_mut(self.world);
        let component_kind = component_ref.kind();
        if !world_data.has_kind(&component_kind) {
            world_data.put_kind::<I>(&component_kind);
        }

        // insert into ecs
        self.world.entity_mut(*entity).insert(component_ref);
    }

    fn remove_component<R: ReplicateSafe<P>>(&mut self, entity: &Entity) -> Option<R> {
        return self.world.entity_mut(*entity).remove::<R>();
    }

    fn remove_component_of_kind(&mut self, entity: &Entity, component_kind: &P::Kind) -> Option<P> {
        let mut output: Option<P> = None;
        self.world
            .resource_scope(|world: &mut World, data: Mut<WorldData<P>>| {
                if let Some(accessor) = data.component_access(component_kind) {
                    output = accessor.remove_component(world, entity);
                }
            });
        output
    }
}

impl<'w, P: Protocolize> ProtocolInserter<P, Entity> for WorldMut<'w> {
    fn insert<I: ReplicateSafe<P>>(&mut self, entity: &Entity, impl_ref: I) {
        self.insert_component::<I>(entity, impl_ref);
    }
}

// private static methods

fn has_entity(world: &World, entity: &Entity) -> bool {
    return world.get_entity(*entity).is_some();
}

fn entities<P: Protocolize>(world: &World) -> Vec<Entity> {
    let world_data = world_data::<P>(world);
    world_data.entities()
}

fn has_component<P: Protocolize, R: ReplicateSafe<P>>(world: &World, entity: &Entity) -> bool {
    return world.get::<R>(*entity).is_some();
}

fn has_component_of_kind<P: Protocolize>(
    world: &World,
    entity: &Entity,
    component_kind: &P::Kind,
) -> bool {
    return world
        .entity(*entity)
        .contains_type_id(component_kind.to_type_id());
}

fn component<'a, P: Protocolize, R: ReplicateSafe<P>>(
    world: &'a World,
    entity: &Entity,
) -> Option<ReplicaRefWrapper<'a, P, R>> {
    if let Some(bevy_ref) = world.get::<R>(*entity) {
        let wrapper = ComponentRef(bevy_ref);
        let component_ref = ReplicaRefWrapper::new(wrapper);
        return Some(component_ref);
    }
    None
}

fn component_of_kind<'a, P: Protocolize>(
    world: &'a World,
    entity: &Entity,
    component_kind: &P::Kind,
) -> Option<ReplicaDynRefWrapper<'a, P>> {
    let world_data = world_data(world);
    if let Some(component_access) = world_data.component_access(component_kind) {
        return component_access.component(world, entity);
    }
    None
}

fn world_data<P: Protocolize>(world: &World) -> &WorldData<P> {
    return world
        .get_resource::<WorldData<P>>()
        .expect("Need to instantiate by adding WorldData<Protocol> resource at startup!");
}

fn world_data_unchecked_mut<P: Protocolize>(world: &World) -> Mut<WorldData<P>> {
    unsafe {
        return world
            .get_resource_unchecked_mut::<WorldData<P>>()
            .expect("Need to instantiate by adding WorldData<Protocol> resource at startup!");
    }
}
