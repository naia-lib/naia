use std::collections::HashMap;

use naia_shared::{
    BigMap, ComponentUpdate, NetEntityHandleConverter, ProtocolInserter, Protocolize,
    ReplicaDynMutWrapper, ReplicaDynRefWrapper, ReplicaMutWrapper, ReplicaRefWrapper, Replicate,
    ReplicateSafe, WorldMutType, WorldRefType,
};

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

impl<P: Protocolize> Default for World<P> {
    fn default() -> Self {
        Self {
            entities: BigMap::default(),
        }
    }
}

impl<P: Protocolize> World<P> {
    /// Convert to WorldRef
    pub fn proxy<'w>(&'w self) -> WorldRef<'w, P> {
        WorldRef::<'w, P>::new(self)
    }

    /// Convert to WorldMut
    pub fn proxy_mut<'w>(&'w mut self) -> WorldMut<'w, P> {
        WorldMut::<'w, P>::new(self)
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
        has_entity(self.world, entity)
    }

    fn entities(&self) -> Vec<Entity> {
        entities(self.world)
    }

    fn has_component<R: ReplicateSafe<P>>(&self, entity: &Entity) -> bool {
        has_component::<P, R>(self.world, entity)
    }

    fn has_component_of_kind(&self, entity: &Entity, component_type: &P::Kind) -> bool {
        has_component_of_type(self.world, entity, component_type)
    }

    fn component<R: ReplicateSafe<P>>(&self, entity: &Entity) -> Option<ReplicaRefWrapper<P, R>> {
        component(self.world, entity)
    }

    fn component_of_kind<'a>(
        &'a self,
        entity: &Entity,
        component_type: &P::Kind,
    ) -> Option<ReplicaDynRefWrapper<'a, P>> {
        component_of_kind(self.world, entity, component_type)
    }
}

impl<'w, P: Protocolize> WorldRefType<P, Entity> for WorldMut<'w, P> {
    fn has_entity(&self, entity: &Entity) -> bool {
        has_entity(self.world, entity)
    }

    fn entities(&self) -> Vec<Entity> {
        entities(self.world)
    }

    fn has_component<R: ReplicateSafe<P>>(&self, entity: &Entity) -> bool {
        has_component::<P, R>(self.world, entity)
    }

    fn has_component_of_kind(&self, entity: &Entity, component_type: &P::Kind) -> bool {
        has_component_of_type(self.world, entity, component_type)
    }

    fn component<R: ReplicateSafe<P>>(&self, entity: &Entity) -> Option<ReplicaRefWrapper<P, R>> {
        component(self.world, entity)
    }

    fn component_of_kind<'a>(
        &'a self,
        entity: &Entity,
        component_type: &P::Kind,
    ) -> Option<ReplicaDynRefWrapper<'a, P>> {
        component_of_kind(self.world, entity, component_type)
    }
}

impl<'w, P: Protocolize> WorldMutType<P, Entity> for WorldMut<'w, P> {
    fn spawn_entity(&mut self) -> Entity {
        let component_map = HashMap::new();
        self.world.entities.insert(component_map)
    }

    fn duplicate_entity(&mut self, entity: &Entity) -> Entity {
        let new_entity = self.spawn_entity();

        self.duplicate_components(&new_entity, entity);

        new_entity
    }

    fn duplicate_components(&mut self, new_entity: &Entity, old_entity: &Entity) {
        for component_kind in self.component_kinds(old_entity) {
            let mut component_copy_opt: Option<P> = None;
            if let Some(component) = self.component_of_kind(old_entity, &component_kind) {
                component_copy_opt = Some(component.protocol_copy());
            }
            if let Some(component_copy) = component_copy_opt {
                Protocolize::extract_and_insert(&component_copy, new_entity, self);
            }
        }
    }

    fn despawn_entity(&mut self, entity: &Entity) {
        self.world.entities.remove(entity);
    }

    fn component_kinds(&mut self, entity: &Entity) -> Vec<P::Kind> {
        let mut output: Vec<P::Kind> = Vec::new();

        if let Some(component_map) = self.world.entities.get(entity) {
            for component_kind in component_map.keys() {
                output.push(*component_kind);
            }
        }

        output
    }

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

        None
    }

    fn component_apply_update(
        &mut self,
        converter: &dyn NetEntityHandleConverter,
        entity: &Entity,
        component_kind: &P::Kind,
        update: ComponentUpdate<P::Kind>,
    ) {
        if let Some(mut component) = component_mut_of_kind(self.world, entity, component_kind) {
            component.read_apply_update(converter, update);
        }
    }

    fn mirror_entities(&mut self, new_entity: &Entity, old_entity: &Entity) {
        for component_kind in self.component_kinds(old_entity) {
            self.mirror_components(new_entity, old_entity, &component_kind);
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
        None
    }

    fn remove_component_of_kind(&mut self, entity: &Entity, component_kind: &P::Kind) -> Option<P> {
        if let Some(component_map) = self.world.entities.get_mut(entity) {
            return component_map.remove(component_kind);
        }

        None
    }
}

impl<'w, P: Protocolize> ProtocolInserter<P, Entity> for WorldMut<'w, P> {
    fn insert<I: ReplicateSafe<P>>(&mut self, entity: &Entity, impl_ref: I) {
        self.insert_component::<I>(entity, impl_ref);
    }
}

// private methods //

fn has_entity<P: Protocolize>(world: &World<P>, entity: &Entity) -> bool {
    world.entities.contains_key(entity)
}

fn entities<P: Protocolize>(world: &World<P>) -> Vec<Entity> {
    let mut output = Vec::new();

    for (key, _) in world.entities.iter() {
        output.push(key);
    }

    output
}

fn has_component<P: Protocolize, R: ReplicateSafe<P>>(world: &World<P>, entity: &Entity) -> bool {
    if let Some(component_map) = world.entities.get(entity) {
        return component_map.contains_key(&Protocolize::kind_of::<R>());
    }

    false
}

fn has_component_of_type<P: Protocolize>(
    world: &World<P>,
    entity: &Entity,
    component_type: &P::Kind,
) -> bool {
    if let Some(component_map) = world.entities.get(entity) {
        return component_map.contains_key(component_type);
    }

    false
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

    None
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

    None
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

    None
}
