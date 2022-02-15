use hecs::{Entity, World};

use naia_shared::{
    DiffMask, PacketReader, ProtocolInserter, Protocolize, ReplicaDynRefWrapper, ReplicaMutWrapper,
    ReplicaRefWrapper, Replicate, ReplicateSafe, WorldMutType, WorldRefType,
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
        return WorldRef::new(self, data);
    }
}

// WorldProxyMut

pub trait WorldProxyMut<'w, 'd, P: Protocolize> {
    fn proxy_mut(self, data: &'d mut WorldData<P>) -> WorldMut<'w, 'd, P>;
}

impl<'w, 'd, P: Protocolize> WorldProxyMut<'w, 'd, P> for &'w mut World {
    fn proxy_mut(self, data: &'d mut WorldData<P>) -> WorldMut<'w, 'd, P> {
        return WorldMut::new(self, data);
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
        return has_entity(self.world, entity);
    }

    fn entities(&self) -> Vec<Entity> {
        return entities(self.world);
    }

    fn has_component<R: ReplicateSafe<P>>(&self, entity: &Entity) -> bool {
        return has_component::<P, R>(self.world, entity);
    }

    fn has_component_of_kind(&self, entity: &Entity, component_kind: &P::Kind) -> bool {
        return has_component_of_kind::<P>(self.world, self.world_data, entity, component_kind);
    }

    fn component<'a, R: ReplicateSafe<P>>(
        &'a self,
        entity: &Entity,
    ) -> Option<ReplicaRefWrapper<'a, P, R>> {
        return component::<P, R>(self.world, entity);
    }

    fn component_of_kind(
        &self,
        entity: &Entity,
        component_kind: &P::Kind,
    ) -> Option<ReplicaDynRefWrapper<'_, P>> {
        return component_of_kind(&self.world, self.world_data, entity, component_kind);
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
        return has_entity(self.world, entity);
    }

    fn entities(&self) -> Vec<Entity> {
        return entities(self.world);
    }

    fn has_component<R: ReplicateSafe<P>>(&self, entity: &Entity) -> bool {
        return has_component::<P, R>(self.world, entity);
    }

    fn has_component_of_kind(&self, entity: &Entity, component_kind: &P::Kind) -> bool {
        return has_component_of_kind::<P>(self.world, self.world_data, entity, component_kind);
    }

    fn component<'a, R: ReplicateSafe<P>>(
        &'a self,
        entity: &Entity,
    ) -> Option<ReplicaRefWrapper<'a, P, R>> {
        return component::<P, R>(self.world, entity);
    }

    fn component_of_kind(
        &self,
        entity: &Entity,
        component_kind: &P::Kind,
    ) -> Option<ReplicaDynRefWrapper<'_, P>> {
        return component_of_kind(self.world, self.world_data, entity, component_kind);
    }
}

impl<'w, 'd, P: Protocolize> WorldMutType<P, Entity> for WorldMut<'w, 'd, P> {
    fn component_mut<'a, R: ReplicateSafe<P>>(
        &'a mut self,
        entity: &Entity,
    ) -> Option<ReplicaMutWrapper<'a, P, R>> {
        if let Ok(hecs_mut) = self.world.get_mut::<R>(*entity) {
            let wrapper = ComponentMut(hecs_mut);
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
        if let Some(access) = self.world_data.component_access(component_kind) {
            if let Some(mut component) = access.component_mut(self.world, entity) {
                component.read_partial(diff_mask, reader, packet_index);
            }
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

    fn component_kinds(&mut self, entity: &Entity) -> Vec<P::Kind> {
        let mut kinds = Vec::new();

        if let Ok(entity_ref) = self.world.entity(*entity) {
            for component_type in entity_ref.component_types() {
                let component_kind = P::type_to_kind(component_type);
                kinds.push(component_kind);
            }
        }

        return kinds;
    }

    fn spawn_entity(&mut self) -> Entity {
        return self.world.spawn(());
    }

    fn despawn_entity(&mut self, entity: &Entity) {
        self.world
            .despawn(*entity)
            .expect("error despawning Entity");
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
        return self.world.remove_one::<R>(*entity).ok();
    }

    fn remove_component_of_kind(&mut self, entity: &Entity, component_kind: &P::Kind) -> Option<P> {
        if let Some(accessor) = self.world_data.component_access(component_kind) {
            return accessor.remove_component(self.world, entity);
        }
        return None;
    }
}

impl<'w, 'd, P: Protocolize> ProtocolInserter<P, Entity> for WorldMut<'w, 'd, P> {
    fn insert<I: ReplicateSafe<P>>(&mut self, entity: &Entity, impl_ref: I) {
        self.insert_component::<I>(entity, impl_ref);
    }
}

// private static methods
fn has_entity(world: &World, entity: &Entity) -> bool {
    return world.contains(*entity);
}

fn entities(world: &World) -> Vec<Entity> {
    let mut output = Vec::new();

    for (entity, _) in world.iter() {
        output.push(entity);
    }

    return output;
}

fn has_component<P: Protocolize, R: ReplicateSafe<P>>(world: &World, entity: &Entity) -> bool {
    let result = world.get::<R>(*entity);
    return result.is_ok();
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
    if let Ok(hecs_ref) = world.get::<R>(*entity) {
        let wrapper = ComponentRef(hecs_ref);
        let component_ref = ReplicaRefWrapper::new(wrapper);
        return Some(component_ref);
    }
    return None;
}

fn component_of_kind<'a, P: Protocolize>(
    world: &'a World,
    world_data: &WorldData<P>,
    entity: &Entity,
    component_kind: &P::Kind,
) -> Option<ReplicaDynRefWrapper<'a, P>> {
    if let Some(access) = world_data.component_access(component_kind) {
        return access.component(world, entity);
    }
    return None;
}
