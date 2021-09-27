use std::{any::TypeId, collections::HashMap};

use slotmap::DenseSlotMap;

use naia_shared::{ImplRef, ProtocolType, Ref, Replicate};

use naia_server::{KeyType, WorldType, ComponentKey};

#[allow(missing_docs)]
#[allow(unused_doc_comments)]
mod entity_key {
    // The Key used to get a reference of a User
    new_key_type! { pub struct EntityKey; }
}

use entity_key::EntityKey;

impl KeyType for EntityKey {}

/// A default World which implements WorldType and that Naia can use to store
/// Entities/Components. It's recommended to use this only when you do not have
/// another ECS library's own World available.
pub struct World<P: ProtocolType> {
    entities: DenseSlotMap<entity_key::EntityKey, HashMap<TypeId, P>>,
}

impl<P: ProtocolType>  World<P> {
    /// Create a new default World
    pub fn new() -> Self {
        World {
            entities: DenseSlotMap::with_key(),
        }
    }
}

impl<P: ProtocolType> WorldType<P> for World<P> {
    type EntityKey = EntityKey;

    fn has_entity(&self, entity_key: &Self::EntityKey) -> bool { todo!() }
    fn has_component_dynamic(&self, entity_key: &Self::EntityKey, component_type: &TypeId) -> bool { todo!() }
    fn get_component_dynamic(&self, entity_key: &Self::EntityKey, component_type: &TypeId) -> Option<P> { todo!() }
    fn get_component_from_key(&self, component_key: &ComponentKey<Self::EntityKey>) -> Option<P> { todo!() }

    fn spawn_entity(&mut self) -> EntityKey {
        let component_map = HashMap::new();
        return self.entities.insert(component_map);
    }

    fn despawn_entity(&mut self, entity_key: &EntityKey) {
        self.entities.remove(*entity_key);
    }

    fn has_component<R: Replicate<P>>(&self, entity_key: &EntityKey) -> bool {
        if let Some(component_map) = self.entities.get(*entity_key) {
            return component_map.contains_key(&TypeId::of::<R>());
        }

        return false;
    }

    fn get_component<R: Replicate<P>>(
        &self,
        entity_key: &EntityKey,
    ) -> Option<Ref<R>> {

        if let Some(component_map) = self.entities.get(*entity_key) {
            if let Some(component_protocol) = component_map.get(&TypeId::of::<R>()){
                return component_protocol.to_typed_ref::<R>();
            }
        }

        return None;
    }

    fn insert_component<R: ImplRef<P>>(
        &mut self,
        entity_key: &EntityKey,
        component_ref: R)
    {
        if let Some(component_map) = self.entities.get_mut(*entity_key) {
            let protocol = component_ref.protocol();
            let type_id = protocol.get_type_id();
            if component_map.contains_key(&type_id) {
                panic!("Entity already has a Component of that type!");
            }
            component_map.insert(type_id, protocol);
        }
    }

    fn remove_component<R: Replicate<P>>(&mut self, entity_key: &EntityKey) {
        if let Some(component_map) = self.entities.get_mut(*entity_key) {
            component_map.remove(&TypeId::of::<R>());
        }
    }

    fn get_components(&self, entity_key: &EntityKey) -> Vec<P> {
        let mut output: Vec<P> = Vec::new();

        if let Some(component_map) = self.entities.get(*entity_key) {
            for (_, component_protocol) in component_map {
                output.push(component_protocol.clone());
            }
        }

        return output;
    }
}