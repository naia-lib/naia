use std::{any::TypeId, collections::HashMap};

use naia_shared::{EntityType, LocalComponentKey, LocalEntity};

#[derive(Debug)]
pub struct EntityRecord<K: EntityType> {
    local_entity: LocalEntity,
    type_to_key_map: HashMap<TypeId, LocalComponentKey>,
    key_to_type_map: HashMap<LocalComponentKey, TypeId>,
    prediction_key: Option<K>,
}

impl<K: EntityType> EntityRecord<K> {
    pub fn new(local_entity: &LocalEntity) -> Self {
        EntityRecord {
            local_entity: *local_entity,
            type_to_key_map: HashMap::new(),
            key_to_type_map: HashMap::new(),
            prediction_key: None,
        }
    }

    pub fn local_entity(&self) -> LocalEntity {
        return self.local_entity;
    }

    // Components / Types //

    //    pub fn get_key_from_type(&self, type_id: &TypeId) ->
    // Option<&LocalComponentKey> {        return
    // self.type_to_key_map.get(type_id);    }

    pub fn get_type_from_key(&self, component_key: &LocalComponentKey) -> Option<&TypeId> {
        return self.key_to_type_map.get(component_key);
    }

    pub fn insert_component(&mut self, key: &LocalComponentKey, type_id: &TypeId) {
        self.type_to_key_map.insert(*type_id, *key);
        self.key_to_type_map.insert(*key, *type_id);
    }

    pub fn remove_component(&mut self, key: &LocalComponentKey) -> Option<TypeId> {
        if let Some(type_id) = self.key_to_type_map.remove(key) {
            self.type_to_key_map.remove(&type_id);
            return Some(type_id);
        }
        return None;
    }

    pub fn get_component_keys(&self) -> Vec<LocalComponentKey> {
        let mut output = Vec::<LocalComponentKey>::new();
        for (key, _) in self.key_to_type_map.iter() {
            output.push(*key);
        }
        return output;
    }

    // Ownership / Prediction //

    pub fn is_owned(&self) -> bool {
        return self.prediction_key.is_some();
    }

    pub fn set_prediction(&mut self, prediction_entity: &K) {
        self.prediction_key = Some(*prediction_entity);
    }

    pub fn disown(&mut self) -> Option<K> {
        return self.prediction_key.take();
    }

    pub fn get_prediction(&self) -> Option<K> {
        return self.prediction_key;
    }
}
