use std::collections::HashMap;

use naia_shared::{serde::BitReader, BigMap, NetEntityHandleConverter, ProtocolInserter, Protocolize, ReplicaDynMutWrapper, ReplicaMutWrapper, ReplicaRefWrapper, Replicate, ReplicateSafe, WorldMutType, WorldRefType, ReplicaDynRefWrapper};

use super::{
    component_ref::{ComponentMut, ComponentRef},
    entity::Entity,
};

// World //

/// A default World which implements WorldRefType/WorldMutType and that Naia can
/// use to store Entities/Components.
/// It's recommended to use this only when you do not have another ECS library's
/// own World available.
pub struct World<P: Protocolize> {
    pub entities: BigMap<Entity, HashMap<P::Kind, P>>,
}

impl<P: Protocolize> World<P> {
    /// Create a new default World
    pub fn new() -> Self {
        World {
            entities: BigMap::new(),
        }
    }

    /// Convert to WorldRef
    pub fn proxy<'w>(&'w self) -> WorldRef<'w, P> {
        return WorldRef::<'w, P>::new(self);
    }

    /// Convert to WorldMut
    pub fn proxy_mut<'w>(&'w mut self) -> WorldMut<'w, P> {
        return WorldMut::<'w, P>::new(self);
    }
}

// WorldRef //

pub struct WorldRef<'w, P: Protocolize> {
    world: &'w World<P>,
}

impl<'w, P: Protocolize> WorldRef<'w, P> {
    pub fn new(world: &'w World<P>) -> Self {
        WorldRef { world }
    }
}

// WorldMut //

pub struct WorldMut<'w, P: Protocolize> {
    world: &'w mut World<P>,
}

impl<'w, P: Protocolize> WorldMut<'w, P> {
    pub fn new(world: &'w mut World<P>) -> Self {
        WorldMut { world }
    }
}

// WorldRefType //

impl<'w, P: Protocolize> WorldRefType<P, Entity> for WorldRef<'w, P> {
    fn has_entity(&self, entity: &Entity) -> bool {
        return has_entity(self.world, entity);
    }

    fn entities(&self) -> Vec<Entity> {
        return entities(self.world);
    }

    fn has_component<R: ReplicateSafe<P>>(&self, entity: &Entity) -> bool {
        return has_component::<P, R>(self.world, entity);
    }

    fn has_component_of_kind(&self, entity: &Entity, component_type: &P::Kind) -> bool {
        return has_component_of_type(self.world, entity, component_type);
    }

    fn component<R: ReplicateSafe<P>>(&self, entity: &Entity) -> Option<ReplicaRefWrapper<P, R>> {
        return component(self.world, entity);
    }

    fn component_of_kind<'a>(&'a self, entity: &Entity, component_type: &P::Kind) -> Option<ReplicaDynRefWrapper<'a, P>> {
        return component_of_kind(self.world, entity, component_type);
    }
}

impl<'w, P: Protocolize> WorldRefType<P, Entity> for WorldMut<'w, P> {
    fn has_entity(&self, entity: &Entity) -> bool {
        return has_entity(self.world, entity);
    }

    fn entities(&self) -> Vec<Entity> {
        return entities(self.world);
    }

    fn has_component<R: ReplicateSafe<P>>(&self, entity: &Entity) -> bool {
        return has_component::<P, R>(self.world, entity);
    }

    fn has_component_of_kind(&self, entity: &Entity, component_type: &P::Kind) -> bool {
        return has_component_of_type(self.world, entity, component_type);
    }

    fn component<R: ReplicateSafe<P>>(&self, entity: &Entity) -> Option<ReplicaRefWrapper<P, R>> {
        return component(self.world, entity);
    }

    fn component_of_kind<'a>(&'a self, entity: &Entity, component_type: &P::Kind) -> Option<ReplicaDynRefWrapper<'a, P>> {
        return component_of_kind(self.world, entity, component_type);
    }
}

impl<'w, P: Protocolize> WorldMutType<P, Entity> for WorldMut<'w, P> {
    fn component_mut<R: ReplicateSafe<P>>(
        &mut self,
        entity: &Entity,
    ) -> Option<ReplicaMutWrapper<P, R>> {
        if let Some(component_map) = self.world.entities.get_mut(entity) {
            if let Some(component_protocol) = component_map.get_mut(&Protocolize::kind_of::<R>()) {
                if let Some(raw_ref) = component_protocol.cast_mut::<R>() {
                    let wrapper = ComponentMut::<P, R>::new(raw_ref);
                    let wrapped_ref = ReplicaMutWrapper::new(wrapper);
                    return Some(wrapped_ref);
                }
            }
        }

        return None;
    }

    fn component_read_partial(
        &mut self,
        entity: &Entity,
        component_kind: &P::Kind,
        reader: &mut BitReader,
        converter: &dyn NetEntityHandleConverter,
    ) {
        if let Some(mut component) = component_mut_of_kind(self.world, entity, component_kind) {
            component.read_partial(reader, converter);
        }
    }

    fn mirror_components(
        &mut self,
        mutable_entity: &Entity,
        immutable_entity: &Entity,
        component_kind: &P::Kind,
    ) {
        let immutable_component_opt: Option<P> = {
            if let Some(immutable_component_map) = self.world.entities.get(immutable_entity) {
                if let Some(immutable_component) = immutable_component_map.get(component_kind) {
                    let immutable_copy = immutable_component.dyn_ref().protocol_copy();
                    Some(immutable_copy)
                } else {
                    None
                }
            } else {
                None
            }
        };
        if let Some(immutable_component) = immutable_component_opt {
            if let Some(mutable_component_map) = self.world.entities.get_mut(mutable_entity) {
                if let Some(mutable_component) = mutable_component_map.get_mut(component_kind) {
                    mutable_component.dyn_mut().mirror(&immutable_component);
                }
            }
        }
    }

    fn component_kinds(&mut self, entity: &Entity) -> Vec<P::Kind> {
        let mut output: Vec<P::Kind> = Vec::new();

        if let Some(component_map) = self.world.entities.get(entity) {
            for (component_kind, _) in component_map {
                output.push(*component_kind);
            }
        }

        return output;
    }

    fn spawn_entity(&mut self) -> Entity {
        let component_map = HashMap::new();
        return self.world.entities.insert(component_map);
    }

    fn despawn_entity(&mut self, entity: &Entity) {
        self.world.entities.remove(entity);
    }

    fn insert_component<R: ReplicateSafe<P>>(&mut self, entity: &Entity, component_ref: R) {
        if let Some(component_map) = self.world.entities.get_mut(entity) {
            let protocol = component_ref.into_protocol();
            let component_kind = Protocolize::kind_of::<R>();
            if component_map.contains_key(&component_kind) {
                panic!("Entity already has a Component of that type!");
            }
            component_map.insert(component_kind, protocol);
        }
    }

    fn remove_component<R: Replicate<P>>(&mut self, entity: &Entity) -> Option<R> {
        if let Some(component_map) = self.world.entities.get_mut(entity) {
            if let Some(protocol) = component_map.remove(&Protocolize::kind_of::<R>()) {
                return protocol.cast::<R>();
            }
        }
        return None;
    }

    fn remove_component_of_kind(&mut self, entity: &Entity, component_kind: &P::Kind) -> Option<P> {
        if let Some(component_map) = self.world.entities.get_mut(entity) {
            return component_map.remove(component_kind);
        }

        return None;
    }
}

impl<'w, P: Protocolize> ProtocolInserter<P, Entity> for WorldMut<'w, P> {
    fn insert<I: ReplicateSafe<P>>(&mut self, entity: &Entity, impl_ref: I) {
        self.insert_component::<I>(entity, impl_ref);
    }
}

// private methods //

fn has_entity<P: Protocolize>(world: &World<P>, entity: &Entity) -> bool {
    return world.entities.contains_key(entity);
}

fn entities<P: Protocolize>(world: &World<P>) -> Vec<Entity> {
    let mut output = Vec::new();

    for (key, _) in world.entities.iter() {
        output.push(key);
    }

    return output;
}

fn has_component<P: Protocolize, R: ReplicateSafe<P>>(world: &World<P>, entity: &Entity) -> bool {
    if let Some(component_map) = world.entities.get(entity) {
        return component_map.contains_key(&Protocolize::kind_of::<R>());
    }

    return false;
}

fn has_component_of_type<P: Protocolize>(
    world: &World<P>,
    entity: &Entity,
    component_type: &P::Kind,
) -> bool {
    if let Some(component_map) = world.entities.get(entity) {
        return component_map.contains_key(component_type);
    }

    return false;
}

fn component<'a, P: Protocolize, R: ReplicateSafe<P>>(
    world: &'a World<P>,
    entity: &Entity,
) -> Option<ReplicaRefWrapper<'a, P, R>> {
    if let Some(component_map) = world.entities.get(entity) {
        if let Some(component_protocol) = component_map.get(&Protocolize::kind_of::<R>()) {
            if let Some(raw_ref) = component_protocol.cast_ref::<R>() {
                let wrapper = ComponentRef::<P, R>::new(raw_ref);
                let wrapped_ref = ReplicaRefWrapper::new(wrapper);
                return Some(wrapped_ref);
            }
        }
    }

    return None;
}

fn component_of_kind<'a, P: Protocolize>(
    world: &'a World<P>,
    entity: &Entity,
    component_type: &P::Kind,
) -> Option<ReplicaDynRefWrapper<'a, P>> {
    if let Some(component_map) = world.entities.get(entity) {
        if let Some(component) = component_map.get(component_type) {
            return Some(ReplicaDynRefWrapper::new(component.dyn_ref()));
        }
    }

    return None;
}

fn component_mut_of_kind<'a, P: Protocolize>(
    world: &'a mut World<P>,
    entity: &Entity,
    component_type: &P::Kind,
) -> Option<ReplicaDynMutWrapper<'a, P>> {
    if let Some(component_map) = world.entities.get_mut(entity) {
        if let Some(raw_ref) = component_map.get_mut(component_type) {
            let wrapped_ref = ReplicaDynMutWrapper::new(raw_ref.dyn_mut());
            return Some(wrapped_ref);
        }
    }

    return None;
}
