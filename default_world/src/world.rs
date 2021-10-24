use std::collections::HashMap;

use slotmap::DenseSlotMap;

use naia_shared::{
    ProtocolInserter, ProtocolType, ReplicaDynMutWrapper, ReplicaDynRefWrapper, ReplicaMutWrapper,
    ReplicaRefWrapper, Replicate, ReplicateSafe, WorldMutType, WorldRefType,
};

use super::{
    component_ref::{MutWrapper, RefWrapper},
    entity::Entity,
};

// World //

/// A default World which implements WorldRefType/WorldMutType and that Naia can
/// use to store Entities/Components.
/// It's recommended to use this only when you do not have another ECS library's
/// own World available.
pub struct World<P: ProtocolType> {
    pub entities: DenseSlotMap<Entity, HashMap<P::Kind, P>>,
}

impl<P: ProtocolType> World<P> {
    /// Create a new default World
    pub fn new() -> Self {
        World {
            entities: DenseSlotMap::with_key(),
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

pub struct WorldRef<'w, P: ProtocolType> {
    world: &'w World<P>,
}

impl<'w, P: ProtocolType> WorldRef<'w, P> {
    pub fn new(world: &'w World<P>) -> Self {
        WorldRef { world }
    }
}

// WorldMut //

pub struct WorldMut<'w, P: ProtocolType> {
    world: &'w mut World<P>,
}

impl<'w, P: ProtocolType> WorldMut<'w, P> {
    pub fn new(world: &'w mut World<P>) -> Self {
        WorldMut { world }
    }
}

// WorldRefType //

impl<'w, P: ProtocolType> WorldRefType<P, Entity> for WorldRef<'w, P> {
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

    fn get_component<R: ReplicateSafe<P>>(
        &self,
        entity: &Entity,
    ) -> Option<ReplicaRefWrapper<P, R>> {
        return get_component(self.world, entity);
    }

    fn get_component_of_kind(
        &self,
        entity: &Entity,
        component_type: &P::Kind,
    ) -> Option<ReplicaDynRefWrapper<'_, P>> {
        return get_component_of_kind(self.world, entity, component_type);
    }
}

impl<'w, P: ProtocolType> WorldRefType<P, Entity> for WorldMut<'w, P> {
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

    fn get_component<R: ReplicateSafe<P>>(
        &self,
        entity: &Entity,
    ) -> Option<ReplicaRefWrapper<P, R>> {
        return get_component(self.world, entity);
    }

    fn get_component_of_kind(
        &self,
        entity: &Entity,
        component_type: &P::Kind,
    ) -> Option<ReplicaDynRefWrapper<'_, P>> {
        return get_component_of_kind(self.world, entity, component_type);
    }
}

impl<'w, P: ProtocolType> WorldMutType<P, Entity> for WorldMut<'w, P> {
    fn get_component_mut<R: ReplicateSafe<P>>(
        &mut self,
        entity: &Entity,
    ) -> Option<ReplicaMutWrapper<P, R>> {
        if let Some(component_map) = self.world.entities.get_mut(*entity) {
            if let Some(component_protocol) = component_map.get_mut(&ProtocolType::kind_of::<R>()) {
                if let Some(raw_ref) = component_protocol.cast_mut::<R>() {
                    let wrapper = MutWrapper::<P, R>::new(raw_ref);
                    let wrapped_ref = ReplicaMutWrapper::new(wrapper);
                    return Some(wrapped_ref);
                }
            }
        }

        return None;
    }

    fn get_component_mut_of_kind(
        &mut self,
        entity: &Entity,
        component_type: &P::Kind,
    ) -> Option<ReplicaDynMutWrapper<'_, P>> {
        if let Some(component_map) = self.world.entities.get_mut(*entity) {
            if let Some(raw_ref) = component_map.get_mut(component_type) {
                let wrapped_ref = ReplicaDynMutWrapper::new(raw_ref.dyn_mut());
                return Some(wrapped_ref);
            }
        }

        return None;
    }

    fn get_component_kinds(&mut self, entity: &Entity) -> Vec<P::Kind> {
        let mut output: Vec<P::Kind> = Vec::new();

        if let Some(component_map) = self.world.entities.get(*entity) {
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
        self.world.entities.remove(*entity);
    }

    fn insert_component<R: ReplicateSafe<P>>(&mut self, entity: &Entity, component_ref: R) {
        if let Some(component_map) = self.world.entities.get_mut(*entity) {
            let protocol = component_ref.into_protocol();
            let component_kind = ProtocolType::kind_of::<R>();
            if component_map.contains_key(&component_kind) {
                panic!("Entity already has a Component of that type!");
            }
            component_map.insert(component_kind, protocol);
        }
    }

    fn remove_component<R: Replicate<P>>(&mut self, entity: &Entity) -> Option<R> {
        if let Some(component_map) = self.world.entities.get_mut(*entity) {
            if let Some(protocol) = component_map.remove(&ProtocolType::kind_of::<R>()) {
                return protocol.cast::<R>();
            }
        }
        return None;
    }

    fn remove_component_of_kind(&mut self, entity: &Entity, component_kind: &P::Kind) -> Option<P> {
        if let Some(component_map) = self.world.entities.get_mut(*entity) {
            return component_map.remove(component_kind);
        }

        return None;
    }
}

impl<'w, P: ProtocolType> ProtocolInserter<P, Entity> for WorldMut<'w, P> {
    fn insert<I: ReplicateSafe<P>>(&mut self, entity: &Entity, impl_ref: I) {
        self.insert_component::<I>(entity, impl_ref);
    }
}

// private methods //

fn has_entity<P: ProtocolType>(world: &World<P>, entity: &Entity) -> bool {
    return world.entities.contains_key(*entity);
}

fn entities<P: ProtocolType>(world: &World<P>) -> Vec<Entity> {
    let mut output = Vec::new();

    for (key, _) in &world.entities {
        output.push(key);
    }

    return output;
}

fn has_component<P: ProtocolType, R: ReplicateSafe<P>>(world: &World<P>, entity: &Entity) -> bool {
    if let Some(component_map) = world.entities.get(*entity) {
        return component_map.contains_key(&ProtocolType::kind_of::<R>());
    }

    return false;
}

fn has_component_of_type<P: ProtocolType>(
    world: &World<P>,
    entity: &Entity,
    component_type: &P::Kind,
) -> bool {
    if let Some(component_map) = world.entities.get(*entity) {
        return component_map.contains_key(component_type);
    }

    return false;
}

fn get_component<'a, P: ProtocolType, R: ReplicateSafe<P>>(
    world: &'a World<P>,
    entity: &Entity,
) -> Option<ReplicaRefWrapper<'a, P, R>> {
    if let Some(component_map) = world.entities.get(*entity) {
        if let Some(component_protocol) = component_map.get(&ProtocolType::kind_of::<R>()) {
            if let Some(raw_ref) = component_protocol.cast_ref::<R>() {
                let wrapper = RefWrapper::<P, R>::new(raw_ref);
                let wrapped_ref = ReplicaRefWrapper::new(wrapper);
                return Some(wrapped_ref);
            }
        }
    }

    return None;
}

fn get_component_of_kind<'a, P: ProtocolType>(
    world: &'a World<P>,
    entity: &Entity,
    component_type: &P::Kind,
) -> Option<ReplicaDynRefWrapper<'a, P>> {
    if let Some(component_map) = world.entities.get(*entity) {
        if let Some(raw_ref) = component_map.get(component_type) {
            let wrapped_ref = ReplicaDynRefWrapper::new(raw_ref.dyn_ref());
            return Some(wrapped_ref);
        }
    }

    return None;
}
