use std::{any::TypeId, collections::HashMap};

use slotmap::DenseSlotMap;

use super::keys::{ComponentKey, KeyType};

pub struct WorldRecord<K: KeyType> {
    entities: HashMap<K, HashMap<TypeId, ComponentKey>>,
    components: DenseSlotMap<ComponentKey, (K, TypeId)>,
}

impl<K: KeyType> WorldRecord<K> {
    pub fn new() -> Self {
        WorldRecord {
            entities: HashMap::new(),
            components: DenseSlotMap::with_key(),
        }
    }

    // Sync w/ World & Server

    pub fn spawn_entity(&mut self, entity_key: &K) {
        if self.entities.contains_key(entity_key) {
            panic!("entity already initialized!");
        }
        self.entities.insert(*entity_key, HashMap::new());
    }

    pub fn despawn_entity(&mut self, entity_key: &K) {
        if !self.entities.contains_key(entity_key) {
            panic!("entity does not exist!");
        }
        let component_key_map = self.entities.get_mut(entity_key).unwrap();

        for (_, component_key) in component_key_map {
            self.components.remove(*component_key);
        }

        self.entities.remove(entity_key);
    }

    pub fn add_component(&mut self, entity_key: &K, component_type: &TypeId) -> ComponentKey {
        if !self.entities.contains_key(entity_key) {
            panic!("entity does not exist!");
        }
        let component_key = self.components.insert((*entity_key, *component_type));
        let component_key_map = self.entities.get_mut(entity_key).unwrap();
        component_key_map.insert(*component_type, component_key);
        return component_key;
    }

    pub fn remove_component(&mut self, component_key: &ComponentKey) {
        if !self.components.contains_key(*component_key) {
            panic!("component does not exist!");
        }

        let (entity_key, type_id) = self.components.remove(*component_key).unwrap();
        if let Some(component_map) = self.entities.get_mut(&entity_key) {
            component_map
                .remove(&type_id)
                .expect("type ids don't match?");
        }
    }

    // Access

    pub fn has_entity(&self, entity_key: &K) -> bool {
        return self.entities.contains_key(entity_key);
    }

    pub fn get_component_keys(&self, entity_key: &K) -> Vec<ComponentKey> {
        let mut output = Vec::new();

        if let Some(component_key_map) = self.entities.get(entity_key) {
            for (_, component_key) in component_key_map {
                output.push(*component_key);
            }
        } else {
            panic!("entity does not exist!");
        }

        output
    }

    pub fn get_key_from_type(&self, entity_key: &K, type_id: &TypeId) -> Option<ComponentKey> {
        if let Some(component_key_map) = self.entities.get(entity_key) {
            if let Some(component_key) = component_key_map.get(type_id) {
                return Some(*component_key);
            }
        }
        return None;
    }

    pub fn get_component_record(&self, component_key: &ComponentKey) -> Option<(K, TypeId)> {
        if let Some(record) = self.components.get(*component_key) {
            return Some(*record);
        }
        return None;
    }
}
