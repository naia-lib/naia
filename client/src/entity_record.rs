use std::collections::HashMap;

use naia_shared::{EntityType, LocalComponentKey, LocalEntity, ProtocolKindType};

#[derive(Debug)]
pub struct EntityRecord<E: EntityType, K: ProtocolKindType> {
    local_entity: LocalEntity,
    kind_to_key_map: HashMap<K, LocalComponentKey>,
    key_to_kind_map: HashMap<LocalComponentKey, K>,
    prediction_key: Option<E>,
}

impl<E: EntityType, K: ProtocolKindType> EntityRecord<E, K> {
    pub fn new(local_entity: &LocalEntity) -> Self {
        EntityRecord {
            local_entity: *local_entity,
            kind_to_key_map: HashMap::new(),
            key_to_kind_map: HashMap::new(),
            prediction_key: None,
        }
    }

    pub fn local_entity(&self) -> LocalEntity {
        return self.local_entity;
    }

    // Components / Kinds //

    pub fn get_kind_from_key(&self, component_key: &LocalComponentKey) -> Option<&K> {
        return self.key_to_kind_map.get(component_key);
    }

    pub fn insert_component(&mut self, key: &LocalComponentKey, kind: &K) {
        self.kind_to_key_map.insert(*kind, *key);
        self.key_to_kind_map.insert(*key, *kind);
    }

    pub fn remove_component(&mut self, key: &LocalComponentKey) -> Option<K> {
        if let Some(kind) = self.key_to_kind_map.remove(key) {
            self.kind_to_key_map.remove(&kind);
            return Some(kind);
        }
        return None;
    }

    pub fn get_component_keys(&self) -> Vec<LocalComponentKey> {
        let mut output = Vec::<LocalComponentKey>::new();
        for (key, _) in self.key_to_kind_map.iter() {
            output.push(*key);
        }
        return output;
    }

    // Ownership / Prediction //

    pub fn is_owned(&self) -> bool {
        return self.prediction_key.is_some();
    }

    pub fn set_prediction(&mut self, prediction_entity: &E) {
        self.prediction_key = Some(*prediction_entity);
    }

    pub fn disown(&mut self) -> Option<E> {
        return self.prediction_key.take();
    }

    pub fn get_prediction(&self) -> Option<E> {
        return self.prediction_key;
    }
}
