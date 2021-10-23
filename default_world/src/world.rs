use std::{
    collections::HashMap,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use slotmap::DenseSlotMap;

use naia_shared::{
    ComponentDynMut, ComponentDynMutTrait, ComponentDynRef, ComponentDynRefTrait, ComponentMut,
    ComponentMutTrait, ComponentRef, ComponentRefTrait, EntityType, ProtocolInserter, ProtocolType,
    Replicate, ReplicateSafe, WorldMutType, WorldRefType,
};

// Entity

#[allow(missing_docs)]
#[allow(unused_doc_comments)]
mod entity {
    // The Key used to reference an Entity
    new_key_type! { pub struct Entity; }
}

use entity::Entity as Key;

pub type Entity = Key;

impl Deref for Entity {
    type Target = Self;

    fn deref(&self) -> &Self {
        &self
    }
}

impl EntityType for Entity {}

// ComponentRefs

struct RefWrapper<'a, P: ProtocolType, R: ReplicateSafe<P>> {
    inner: &'a R,
    phantom: PhantomData<P>,
}

struct MutWrapper<'a, P: ProtocolType, R: ReplicateSafe<P>> {
    inner: &'a mut R,
    phantom: PhantomData<P>,
}

struct DynRefWrapper<'a, P: ProtocolType> {
    inner: &'a dyn ReplicateSafe<P>,
}

struct DynMutWrapper<'a, P: ProtocolType> {
    inner: &'a mut dyn ReplicateSafe<P>,
}

impl<'a, P: ProtocolType, R: ReplicateSafe<P>> RefWrapper<'a, P, R> {
    pub fn new(inner: &'a R) -> Self {
        Self {
            inner,
            phantom: PhantomData,
        }
    }
}

impl<'a, P: ProtocolType, R: ReplicateSafe<P>> MutWrapper<'a, P, R> {
    pub fn new(inner: &'a mut R) -> Self {
        Self {
            inner,
            phantom: PhantomData,
        }
    }
}

impl<'a, P: ProtocolType> DynRefWrapper<'a, P> {
    pub fn new(inner: &'a dyn ReplicateSafe<P>) -> Self {
        Self { inner }
    }
}

impl<'a, P: ProtocolType> DynMutWrapper<'a, P> {
    pub fn new(inner: &'a mut dyn ReplicateSafe<P>) -> Self {
        Self { inner }
    }
}

impl<'a, P: ProtocolType, R: ReplicateSafe<P>> ComponentRefTrait<P, R> for RefWrapper<'a, P, R> {
    fn component_deref(&self) -> &R {
        return &self.inner;
    }
}

impl<'a, P: ProtocolType, R: ReplicateSafe<P>> ComponentRefTrait<P, R> for MutWrapper<'a, P, R> {
    fn component_deref(&self) -> &R {
        return &self.inner;
    }
}

impl<'a, P: ProtocolType, R: ReplicateSafe<P>> ComponentMutTrait<P, R> for MutWrapper<'a, P, R> {
    fn component_deref_mut(&mut self) -> &mut R {
        return &mut self.inner;
    }
}

impl<'a, P: ProtocolType> ComponentDynRefTrait<P> for DynRefWrapper<'a, P> {
    fn component_dyn_deref(&self) -> &dyn ReplicateSafe<P> {
        return self.inner;
    }
}

impl<'a, P: ProtocolType> ComponentDynRefTrait<P> for DynMutWrapper<'a, P> {
    fn component_dyn_deref(&self) -> &dyn ReplicateSafe<P> {
        return self.inner;
    }
}

impl<'a, P: ProtocolType> ComponentDynMutTrait<P> for DynMutWrapper<'a, P> {
    fn component_dyn_deref_mut(&mut self) -> &mut dyn ReplicateSafe<P> {
        return self.inner;
    }
}

// World //

/// A default World which implements WorldRefType/WorldMutType and that Naia can
/// use to store Entities/Components.
/// It's recommended to use this only when you do not have another ECS library's
/// own World available.
pub struct World<P: ProtocolType> {
    pub entities: DenseSlotMap<entity::Entity, HashMap<P::Kind, P>>,
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

    fn get_component<R: ReplicateSafe<P>>(&self, entity: &Entity) -> Option<ComponentRef<P, R>> {
        return get_component(self.world, entity);
    }

    fn get_component_of_kind(
        &self,
        entity: &Entity,
        component_type: &P::Kind,
    ) -> Option<ComponentDynRef<'_, P>> {
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

    fn get_component<R: ReplicateSafe<P>>(&self, entity: &Entity) -> Option<ComponentRef<P, R>> {
        return get_component(self.world, entity);
    }

    fn get_component_of_kind(
        &self,
        entity: &Entity,
        component_type: &P::Kind,
    ) -> Option<ComponentDynRef<'_, P>> {
        return get_component_of_kind(self.world, entity, component_type);
    }
}

impl<'w, P: ProtocolType> WorldMutType<P, Entity> for WorldMut<'w, P> {
    fn get_component_mut<R: ReplicateSafe<P>>(
        &mut self,
        entity: &Entity,
    ) -> Option<ComponentMut<P, R>> {
        if let Some(component_map) = self.world.entities.get_mut(*entity) {
            if let Some(component_protocol) = component_map.get_mut(&ProtocolType::kind_of::<R>()) {
                if let Some(raw_ref) = component_protocol.cast_mut::<R>() {
                    let wrapper = MutWrapper::<P, R>::new(raw_ref);
                    let wrapped_ref = ComponentMut::new(wrapper);
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
    ) -> Option<ComponentDynMut<'_, P>> {
        if let Some(component_map) = self.world.entities.get_mut(*entity) {
            if let Some(raw_ref) = component_map.get_mut(component_type) {
                let wrapped_ref = ComponentDynMut::new(raw_ref.dyn_mut());
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
) -> Option<ComponentRef<'a, P, R>> {
    if let Some(component_map) = world.entities.get(*entity) {
        if let Some(component_protocol) = component_map.get(&ProtocolType::kind_of::<R>()) {
            if let Some(raw_ref) = component_protocol.cast_ref::<R>() {
                let wrapper = RefWrapper::<P, R>::new(raw_ref);
                let wrapped_ref = ComponentRef::new(wrapper);
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
) -> Option<ComponentDynRef<'a, P>> {
    if let Some(component_map) = world.entities.get(*entity) {
        if let Some(raw_ref) = component_map.get_mut(component_type) {
            let wrapped_ref = ComponentDynRef::new(raw_ref.dyn_ref());
            return Some(wrapped_ref);
        }
    }

    return None;
}
