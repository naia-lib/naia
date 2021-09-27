use std::any::TypeId;

use slotmap::{DenseSlotMap, Key};

use naia_shared::{ImplRef, ProtocolType, Ref, Replicate};

use naia_server::{KeyType, WorldType};

/// A default World which implements WorldType and that Naia can use to store
/// Entities/Components. It's recommended to use this only when you do not have
/// another ECS library's own World available.
pub struct World<P: ProtocolType> {
    entities: DenseSlotMap<EntityKey, HashMap<TypeId, P>>,
}

impl<P: ProtocolType>  World<P> {
    /// Create a new default World
    pub fn new() -> Self {
        World {
            entities: DenseSlotMap::with_key(),
        }
    }
}

impl<P: ProtocolType> WorldType<P, EntityKey> for World<P> {
    fn spawn_entity(&mut self) -> EntityKey {
        let component_map = HashMap::new();
        return self.entities.insert(component_map);
    }

    fn despawn_entity(&mut self, entity_key: &EntityKey) {
        self.entities.remove(entity_key);
    }

    fn has_component<R: Replicate<P>>(&self, entity_key: &EntityKey) -> bool {
        if let Some(component_map) = self.entities.get(entity_key) {
            return component_map.contains_key(TypeId::of::<R>());
        }

        return false;
    }

    fn get_component<R: Replicate<P>>(
        &self,
        entity_key: &EntityKey,
    ) -> Option<Ref<R>> {

        if let Some(component_map) = self.entities.get(entity_key) {
            if let Some(component_ref) = component_map.get(TypeId::of::<R>()){
                return Some(component_ref.to_typed_ref());
            }
        }

        return None;
    }

    fn insert_component<R: ImplRef<P>>(
        &mut self,
        entity_key: &EntityKey,
        component_ref: R)
    {
        if let Some(component_map) = self.entities.get_mut(entity_key) {
            let protocol = component_ref.protocol();
            let type_id = protocol.get_type_id();
            if component_map.contains_key(type_id) {
                panic!("Entity already has a Component of that type!");
            }
            component_map.insert(type_id, protocol);
        }
    }

    fn remove_component<R: Replicate<P>>(&mut self, entity_key: &EntityKey) {
        if let Some(component_map) = self.entities.get_mut(entity_key) {
            let type_id = TypeId::of::<R>();
            component_map.remove(TypeId::of::<R>());
        }
    }

    fn get_components(&self, entity_key: &EntityKey) -> Vec<P> {

    }
}

// Keys

#[allow(missing_docs)]
#[allow(unused_doc_comments)]
mod entity_key {
    // The Global Key used to get a reference of a Entity
    new_key_type! { struct EntityKey; }
}