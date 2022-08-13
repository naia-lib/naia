use hecs::{Entity, World};

use naia_shared::{
    ComponentUpdate, NetEntityHandleConverter, ProtocolInserter, Protocolize, ReplicaDynRefWrapper,
    ReplicaMutWrapper, ReplicaRefWrapper, Replicate, ReplicateSafe, WorldMutType, WorldRefType,
};

use super::{
    component_ref::{ComponentMut, ComponentRef},
    world_data::WorldData,
};

// WorldProxy

pub trait WorldProxy<'w, 'd, P: Protocolize> {
    fn proxy(self, data: &'d WorldData<P>) -> WorldRef<'w, 'd, P>;
}

impl<'w, 'd, P: Protocolize> WorldProxy<'w, 'd, P> for &'w World {
    fn proxy(self, data: &'d WorldData<P>) -> WorldRef<'w, 'd, P> {
        WorldRef::new(self, data)
    }
}

// WorldProxyMut

pub trait WorldProxyMut<'w, 'd, P: Protocolize> {
    fn proxy_mut(self, data: &'d mut WorldData<P>) -> WorldMut<'w, 'd, P>;
}

impl<'w, 'd, P: Protocolize> WorldProxyMut<'w, 'd, P> for &'w mut World {
    fn proxy_mut(self, data: &'d mut WorldData<P>) -> WorldMut<'w, 'd, P> {
        WorldMut::new(self, data)
    }
}

// WorldRef

pub struct WorldRef<'w, 'd, P: Protocolize> {
    world: &'w World,
    world_data: &'d WorldData<P>,
}

impl<'w, 'd, P: Protocolize> WorldRef<'w, 'd, P> {
    pub fn new(world: &'w World, data: &'d WorldData<P>) -> Self {
        WorldRef {
            world,
            world_data: data,
        }
    }
}

impl<'w, 'd, P: Protocolize> WorldRefType<P, Entity> for WorldRef<'w, 'd, P> {
    fn has_entity(&self, entity: &Entity) -> bool {
        has_entity(self.world, entity)
    }

    fn entities(&self) -> Vec<Entity> {
        entities(self.world)
    }

    fn has_component<R: ReplicateSafe<P>>(&self, entity: &Entity) -> bool {
        has_component::<P, R>(self.world, entity)
    }

    fn has_component_of_kind(&self, entity: &Entity, component_kind: &P::Kind) -> bool {
        has_component_of_kind::<P>(self.world, self.world_data, entity, component_kind)
    }

    fn component<R: ReplicateSafe<P>>(&self, entity: &Entity) -> Option<ReplicaRefWrapper<P, R>> {
        component::<P, R>(self.world, entity)
    }

    fn component_of_kind<'a>(
        &'a self,
        entity: &Entity,
        component_kind: &P::Kind,
    ) -> Option<ReplicaDynRefWrapper<'a, P>> {
        component_of_kind(self.world, self.world_data, entity, component_kind)
    }
}

// WorldMut

pub struct WorldMut<'w, 'd, P: Protocolize> {
    world: &'w mut World,
    world_data: &'d mut WorldData<P>,
}

impl<'w, 'd, P: Protocolize> WorldMut<'w, 'd, P> {
    pub fn new(world: &'w mut World, data: &'d mut WorldData<P>) -> Self {
        WorldMut {
            world,
            world_data: data,
        }
    }
}

impl<'w, 'd, P: Protocolize> WorldRefType<P, Entity> for WorldMut<'w, 'd, P> {
    fn has_entity(&self, entity: &Entity) -> bool {
        has_entity(self.world, entity)
    }

    fn entities(&self) -> Vec<Entity> {
        entities(self.world)
    }

    fn has_component<R: ReplicateSafe<P>>(&self, entity: &Entity) -> bool {
        has_component::<P, R>(self.world, entity)
    }

    fn has_component_of_kind(&self, entity: &Entity, component_kind: &P::Kind) -> bool {
        has_component_of_kind::<P>(self.world, self.world_data, entity, component_kind)
    }

    fn component<R: ReplicateSafe<P>>(&self, entity: &Entity) -> Option<ReplicaRefWrapper<P, R>> {
        component::<P, R>(self.world, entity)
    }

    fn component_of_kind<'a>(
        &'a self,
        entity: &Entity,
        component_kind: &P::Kind,
    ) -> Option<ReplicaDynRefWrapper<'a, P>> {
        component_of_kind(self.world, self.world_data, entity, component_kind)
    }
}

impl<'w, 'd, P: Protocolize> WorldMutType<P, Entity> for WorldMut<'w, 'd, P> {
    fn spawn_entity(&mut self) -> Entity {
        self.world.spawn(())
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
        self.world
            .despawn(*entity)
            .expect("error despawning Entity");
    }

    fn component_kinds(&mut self, entity: &Entity) -> Vec<P::Kind> {
        let mut kinds = Vec::new();

        if let Ok(entity_ref) = self.world.entity(*entity) {
            for component_type in entity_ref.component_types() {
                if let Some(component_kind) = P::type_to_kind(component_type) {
                    kinds.push(component_kind);
                }
            }
        }

        kinds
    }

    fn component_mut<R: ReplicateSafe<P>>(
        &mut self,
        entity: &Entity,
    ) -> Option<ReplicaMutWrapper<P, R>> {
        if let Ok(hecs_mut) = self.world.get::<&mut R>(*entity) {
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
        component_kind: &P::Kind,
        update: ComponentUpdate<P::Kind>,
    ) {
        if let Some(access) = self.world_data.component_access(component_kind) {
            if let Some(mut component) = access.component_mut(self.world, entity) {
                component.read_apply_update(converter, update);
            }
        }
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
        if let Some(accessor) = self.world_data.component_access(component_kind) {
            accessor.mirror_components(self.world, mutable_entity, immutable_entity);
        }
    }

    fn insert_component<R: ReplicateSafe<P>>(&mut self, entity: &Entity, component_ref: R) {
        // cache type id for later
        // todo: can we initialize this map on startup via Protocol derive?
        let component_kind = component_ref.kind();
        if !self.world_data.has_kind(&component_kind) {
            self.world_data.put_kind::<R>(&component_kind);
        }

        self.world
            .insert_one(*entity, component_ref)
            .expect("error inserting Component");
    }

    fn remove_component<R: Replicate<P>>(&mut self, entity: &Entity) -> Option<R> {
        self.world.remove_one::<R>(*entity).ok()
    }

    fn remove_component_of_kind(&mut self, entity: &Entity, component_kind: &P::Kind) -> Option<P> {
        if let Some(accessor) = self.world_data.component_access(component_kind) {
            return accessor.remove_component(self.world, entity);
        }
        None
    }
}

impl<'w, 'd, P: Protocolize> ProtocolInserter<P, Entity> for WorldMut<'w, 'd, P> {
    fn insert<I: ReplicateSafe<P>>(&mut self, entity: &Entity, impl_ref: I) {
        self.insert_component::<I>(entity, impl_ref);
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

fn has_component<P: Protocolize, R: ReplicateSafe<P>>(world: &World, entity: &Entity) -> bool {
    let result = world.get::<&R>(*entity);
    result.is_ok()
}

fn has_component_of_kind<P: Protocolize>(
    world: &World,
    world_data: &WorldData<P>,
    entity: &Entity,
    component_kind: &P::Kind,
) -> bool {
    return component_of_kind::<P>(world, world_data, entity, component_kind).is_some();
}

fn component<'a, P: Protocolize, R: ReplicateSafe<P>>(
    world: &'a World,
    entity: &Entity,
) -> Option<ReplicaRefWrapper<'a, P, R>> {
    if let Ok(hecs_ref) = world.get::<&R>(*entity) {
        let wrapper = ComponentRef(hecs_ref);
        let component_ref = ReplicaRefWrapper::new(wrapper);
        return Some(component_ref);
    }
    None
}

fn component_of_kind<'a, P: Protocolize>(
    world: &'a World,
    world_data: &'a WorldData<P>,
    entity: &Entity,
    component_kind: &P::Kind,
) -> Option<ReplicaDynRefWrapper<'a, P>> {
    if let Some(access) = world_data.component_access(component_kind) {
        return access.component(world, entity);
    }
    None
}
