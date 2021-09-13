use std::{any::TypeId, collections::HashMap, hash::Hash};

/// Keeps a record of Components, their Keys and TypeIds, for a given Entity
#[derive(Debug)]
pub struct ComponentRecord<K: Eq + Hash + Copy> {
    key_set_ref: HashMap<K, TypeId>,
    type_map: HashMap<TypeId, K>,
}

impl<K: Eq + Hash + Copy> ComponentRecord<K> {
    /// Create a new ComponentRecord
    pub fn new() -> Self {
        ComponentRecord {
            key_set_ref: HashMap::new(),
            type_map: HashMap::new(),
        }
    }

    /// Gets a ComponentKey for a specific associated Type, if it exists
    pub fn get_key_from_type(&self, type_id: &TypeId) -> Option<&K> {
        return self.type_map.get(type_id);
    }

    /// Inserts a new Component into the ComponentRecord
    pub fn insert_component(&mut self, key: &K, type_id: &TypeId) {
        self.key_set_ref.insert(*key, *type_id);
        if self.type_map.contains_key(type_id) {
            panic!("duplicate component types in entity!");
        }
        self.type_map.insert(*type_id, *key);
    }

    /// Removes a Component from the ComponentRecord
    pub fn remove_component(&mut self, key: &K) {
        if let Some(type_id) = self.key_set_ref.remove(key) {
            self.type_map.remove(&type_id);
        }
    }

    /// Gets a list of ComponentKeys in the ComponentRecord
    pub fn get_component_keys(&self) -> Vec<K> {
        //TODO: make more efficient!!!
        let mut output = Vec::<K>::new();
        for key in self.key_set_ref.keys() {
            output.push(*key);
        }
        return output;
    }
}
