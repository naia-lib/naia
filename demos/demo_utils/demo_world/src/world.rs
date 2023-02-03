use std::collections::HashMap;

use naia_shared::{
    serde::SerdeErr, BigMap, ComponentUpdate, NetEntityHandleConverter,
    ReplicaDynMutWrapper, ReplicaDynRefWrapper, ReplicaMutWrapper, ReplicaRefWrapper,
    Replicate, ReplicateSafe, WorldMutType, WorldRefType,
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
pub struct World {
    pub entities: BigMap<Entity, HashMap<ComponentId, Box<dyn ReplicateSafe>>>,
}

impl Default for World {
    fn default() -> Self {
        Self {
            entities: BigMap::default(),
        }
    }
}

impl World {
    /// Convert to WorldRef
    pub fn proxy<'w>(&'w self) -> WorldRef<'w> {
        WorldRef::<'w>::new(self)
    }

    /// Convert to WorldMut
    pub fn proxy_mut<'w>(&'w mut self) -> WorldMut<'w> {
        WorldMut::<'w>::new(self)
    }
}

// WorldRef //

pub struct WorldRef<'w> {
    world: &'w World,
}

impl<'w, P: Protocolize> WorldRef<'w> {
    pub fn new(world: &'w World) -> Self {
        WorldRef { world }
    }
}

// WorldMut //

pub struct WorldMut<'w> {
    world: &'w mut World,
}

impl<'w> WorldMut<'w> {
    pub fn new(world: &'w mut World) -> Self {
        WorldMut { world }
    }
}

// WorldRefType //

impl<'w> WorldRefType<Entity> for WorldRef<'w> {
    fn has_entity(&self, entity: &Entity) -> bool {
        has_entity(self.world, entity)
    }

    fn entities(&self) -> Vec<Entity> {
        entities(self.world)
    }

    fn has_component<R: ReplicateSafe>(&self, entity: &Entity) -> bool {
        has_component::<R>(self.world, entity)
    }

    fn has_component_of_kind(&self, entity: &Entity, component_type: &ComponentId) -> bool {
        has_component_of_type(self.world, entity, component_type)
    }

    fn component<R: ReplicateSafe>(&self, entity: &Entity) -> Option<ReplicaRefWrapper<R>> {
        component(self.world, entity)
    }

    fn component_of_kind<'a>(
        &'a self,
        entity: &Entity,
        component_type: &ComponentId,
    ) -> Option<ReplicaDynRefWrapper<'a>> {
        component_of_kind(self.world, entity, component_type)
    }
}

impl<'w> WorldRefType<Entity> for WorldMut<'w> {
    fn has_entity(&self, entity: &Entity) -> bool {
        has_entity(self.world, entity)
    }

    fn entities(&self) -> Vec<Entity> {
        entities(self.world)
    }

    fn has_component<R: ReplicateSafe>(&self, entity: &Entity) -> bool {
        has_component::<P, R>(self.world, entity)
    }

    fn has_component_of_kind(&self, entity: &Entity, component_type: &ComponentId) -> bool {
        has_component_of_type(self.world, entity, component_type)
    }

    fn component<R: ReplicateSafe>(&self, entity: &Entity) -> Option<ReplicaRefWrapper<R>> {
        component(self.world, entity)
    }

    fn component_of_kind<'a>(
        &'a self,
        entity: &Entity,
        component_type: &ComponentId,
    ) -> Option<ReplicaDynRefWrapper<'a>> {
        component_of_kind(self.world, entity, component_type)
    }
}

impl<'w> WorldMutType<Entity> for WorldMut<'w> {
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
                todo!()
                //Protocolize::extract_and_insert(&component_copy, new_entity, self);
            }
        }
    }

    fn despawn_entity(&mut self, entity: &Entity) {
        self.world.entities.remove(entity);
    }

    fn component_kinds(&mut self, entity: &Entity) -> Vec<ComponentId> {
        let mut output: Vec<ComponentId> = Vec::new();

        if let Some(component_map) = self.world.entities.get(entity) {
            for component_kind in component_map.keys() {
                output.push(*component_kind);
            }
        }

        output
    }

    fn component_mut<R: ReplicateSafe>(
        &mut self,
        entity: &Entity,
    ) -> Option<ReplicaMutWrapper<R>> {
        if let Some(component_map) = self.world.entities.get_mut(entity) {
            if let Some(component_protocol) = component_map.get_mut(&Protocolize::kind_of::<R>()) {
                if let Some(raw_ref) = component_protocol.cast_mut::<R>() {
                    let wrapper = ComponentMut::<R>::new(raw_ref);
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
        component_kind: &ComponentId,
        update: ComponentUpdate,
    ) -> Result<(), SerdeErr> {
        if let Some(mut component) = component_mut_of_kind(self.world, entity, component_kind) {
            component.read_apply_update(converter, update)?;
        }
        Ok(())
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

    fn insert_component<R: ReplicateSafe>(&mut self, entity: &Entity, component_ref: R) {
        if let Some(component_map) = self.world.entities.get_mut(entity) {
            let protocol = component_ref.into_protocol();
            let component_kind = Protocolize::kind_of::<R>();
            if component_map.contains_key(&component_kind) {
                panic!("Entity already has a Component of that type!");
            }
            component_map.insert(component_kind, protocol);
        }
    }

    fn remove_component<R: Replicate>(&mut self, entity: &Entity) -> Option<R> {
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

impl<'w> ProtocolInserter<Entity> for WorldMut<'w> {
    fn insert<I: ReplicateSafe>(&mut self, entity: &Entity, impl_ref: I) {
        self.insert_component::<I>(entity, impl_ref);
    }
}

// private methods //

fn has_entity(world: &World, entity: &Entity) -> bool {
    world.entities.contains_key(entity)
}

fn entities(world: &World) -> Vec<Entity> {
    let mut output = Vec::new();

    for (key, _) in world.entities.iter() {
        output.push(key);
    }

    output
}

fn has_component<R: ReplicateSafe>(world: &World, entity: &Entity) -> bool {
    if let Some(component_map) = world.entities.get(entity) {
        return component_map.contains_key(&Protocolize::kind_of::<R>());
    }

    false
}

fn has_component_of_type(
    world: &World,
    entity: &Entity,
    component_type: &ComponentId,
) -> bool {
    if let Some(component_map) = world.entities.get(entity) {
        return component_map.contains_key(component_type);
    }

    false
}

fn component<'a, R: ReplicateSafe>(
    world: &'a World,
    entity: &Entity,
) -> Option<ReplicaRefWrapper<'a, R>> {
    if let Some(component_map) = world.entities.get(entity) {
        if let Some(component_protocol) = component_map.get(&Protocolize::kind_of::<R>()) {
            if let Some(raw_ref) = component_protocol.cast_ref::<R>() {
                let wrapper = ComponentRef::<R>::new(raw_ref);
                let wrapped_ref = ReplicaRefWrapper::new(wrapper);
                return Some(wrapped_ref);
            }
        }
    }

    None
}

fn component_of_kind<'a>(
    world: &'a World,
    entity: &Entity,
    component_type: &ComponentId,
) -> Option<ReplicaDynRefWrapper<'a>> {
    if let Some(component_map) = world.entities.get(entity) {
        if let Some(component) = component_map.get(component_type) {
            return Some(ReplicaDynRefWrapper::new(component.dyn_ref()));
        }
    }

    None
}

fn component_mut_of_kind<'a>(
    world: &'a mut World,
    entity: &Entity,
    component_type: &ComponentId,
) -> Option<ReplicaDynMutWrapper<'a>> {
    if let Some(component_map) = world.entities.get_mut(entity) {
        if let Some(raw_ref) = component_map.get_mut(component_type) {
            let wrapped_ref = ReplicaDynMutWrapper::new(raw_ref.dyn_mut());
            return Some(wrapped_ref);
        }
    }

    None
}
