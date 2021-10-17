use std::{any::TypeId, collections::HashMap, ops::Deref};

use slotmap::DenseSlotMap;

use naia_shared::{
    EntityType, ProtocolType, Ref, Replicate, WorldMutType,
    WorldRefType,
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

// World //

/// A default World which implements WorldRefType/WorldMutType and that Naia can
/// use to store Entities/Components.
/// It's recommended to use this only when you do not have another ECS library's
/// own World available.
pub struct World<P: ProtocolType> {
    pub entities: DenseSlotMap<entity::Entity, HashMap<TypeId, P>>,
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

    fn has_component<R: Replicate<P>>(&self, entity: &Entity) -> bool {
        return has_component::<P, R>(self.world, entity);
    }

    fn has_component_of_type(&self, entity: &Entity, component_type: &TypeId) -> bool {
        return has_component_of_type(self.world, entity, component_type);
    }

    fn get_component<R: Replicate<P>>(&self, entity: &Entity) -> Option<Ref<R>> {
        return get_component(self.world, entity);
    }

    fn get_component_from_type(&self, entity: &Entity, component_type: &TypeId) -> Option<P> {
        return get_component_from_type(self.world, entity, component_type);
    }
}

impl<'w, P: ProtocolType> WorldRefType<P, Entity> for WorldMut<'w, P> {
    fn has_entity(&self, entity: &Entity) -> bool {
        return has_entity(self.world, entity);
    }

    fn entities(&self) -> Vec<Entity> {
        return entities(self.world);
    }

    fn has_component<R: Replicate<P>>(&self, entity: &Entity) -> bool {
        return has_component::<P, R>(self.world, entity);
    }

    fn has_component_of_type(&self, entity: &Entity, component_type: &TypeId) -> bool {
        return has_component_of_type(self.world, entity, component_type);
    }

    fn get_component<R: Replicate<P>>(&self, entity: &Entity) -> Option<Ref<R>> {
        return get_component(self.world, entity);
    }

    fn get_component_from_type(&self, entity: &Entity, component_type: &TypeId) -> Option<P> {
        return get_component_from_type(self.world, entity, component_type);
    }
}

impl<'w, P: ProtocolType> WorldMutType<P, Entity> for WorldMut<'w, P> {
    fn get_components(&mut self, entity: &Entity) -> Vec<P> {
        let mut output: Vec<P> = Vec::new();

        if let Some(component_map) = self.world.entities.get(*entity) {
            for (_, component_protocol) in component_map {
                output.push(component_protocol.clone());
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

    fn insert_component<R: Replicate<P>>(&mut self, entity: &Entity, component_ref: R) {
        if let Some(component_map) = self.world.entities.get_mut(*entity) {
            let protocol = component_ref.protocol();
            let type_id = protocol.get_type_id();
            if component_map.contains_key(&type_id) {
                panic!("Entity already has a Component of that type!");
            }
            component_map.insert(type_id, protocol);
        }
    }

    fn remove_component<R: Replicate<P>>(&mut self, entity: &Entity) {
        if let Some(component_map) = self.world.entities.get_mut(*entity) {
            component_map.remove(&TypeId::of::<R>());
        }
    }

    fn remove_component_by_type(&mut self, entity: &Entity, type_id: &TypeId) {
        if let Some(component_map) = self.world.entities.get_mut(*entity) {
            component_map.remove(type_id);
        }
    }
}

//impl<'w, P: ProtocolType> ProtocolExtractor<P, Entity> for WorldMut<'w, P> {
//    fn extract<I: Replicate<P>>(&mut self, entity: &Entity, impl_ref: I) {
//        self.insert_component::<I>(entity, impl_ref);
//    }
//}

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

fn has_component<P: ProtocolType, R: Replicate<P>>(world: &World<P>, entity: &Entity) -> bool {
    if let Some(component_map) = world.entities.get(*entity) {
        return component_map.contains_key(&TypeId::of::<R>());
    }

    return false;
}

fn has_component_of_type<P: ProtocolType>(
    world: &World<P>,
    entity: &Entity,
    component_type: &TypeId,
) -> bool {
    if let Some(component_map) = world.entities.get(*entity) {
        return component_map.contains_key(component_type);
    }

    return false;
}

fn get_component<P: ProtocolType, R: Replicate<P>>(
    world: &World<P>,
    entity: &Entity,
) -> Option<Ref<R>> {
    if let Some(component_map) = world.entities.get(*entity) {
        if let Some(component_protocol) = component_map.get(&TypeId::of::<R>()) {
            return component_protocol.to_typed_ref::<R>();
        }
    }

    return None;
}

fn get_component_from_type<P: ProtocolType>(
    world: &World<P>,
    entity: &Entity,
    component_type: &TypeId,
) -> Option<P> {
    if let Some(component_map) = world.entities.get(*entity) {
        if let Some(protocol) = component_map.get(component_type) {
            return Some(protocol.clone());
        }
    }

    return None;
}
