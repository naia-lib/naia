use std::{collections::HashMap, hash::Hash};

use slotmap::DenseSlotMap;

use naia_shared::ProtocolKindType;

use super::keys::ComponentKey;

pub struct WorldRecord<E: Copy + Eq + Hash, K: ProtocolKindType> {
    entities: HashMap<E, HashMap<K, ComponentKey>>,
    components: DenseSlotMap<ComponentKey, (E, K)>,
}

impl<E: Copy + Eq + Hash, K: ProtocolKindType> WorldRecord<E, K> {
    pub fn new() -> Self {
        WorldRecord {
            entities: HashMap::new(),
            components: DenseSlotMap::with_key(),
        }
    }

    // Sync w/ World & Server

    pub fn spawn_entity(&mut self, entity: &E) {
        if self.entities.contains_key(entity) {
            panic!("entity already initialized!");
        }
        self.entities.insert(*entity, HashMap::new());
    }

    pub fn despawn_entity(&mut self, entity: &E) {
        if !self.entities.contains_key(entity) {
            panic!("entity does not exist!");
        }
        let component_key_map = self.entities.get_mut(entity).unwrap();

        for (_, component_key) in component_key_map {
            self.components.remove(*component_key);
        }

        self.entities.remove(entity);
    }

    pub fn add_component(&mut self, entity: &E, component_type: &K) -> ComponentKey {
        if !self.entities.contains_key(entity) {
            panic!("entity does not exist!");
        }
        let component_key = self.components.insert((*entity, *component_type));
        let component_key_map = self.entities.get_mut(entity).unwrap();
        component_key_map.insert(*component_type, component_key);
        return component_key;
    }

    pub fn remove_component(&mut self, component_key: &ComponentKey) {
        if !self.components.contains_key(*component_key) {
            panic!("component does not exist!");
        }

        let (entity, component_kind) = self.components.remove(*component_key).unwrap();
        if let Some(component_map) = self.entities.get_mut(&entity) {
            component_map
                .remove(&component_kind)
                .expect("type ids don't match?");
        }
    }

    // Access

    pub fn has_entity(&self, entity: &E) -> bool {
        return self.entities.contains_key(entity);
    }

    pub fn get_component_keys(&self, entity: &E) -> Vec<ComponentKey> {
        let mut output = Vec::new();

        if let Some(component_key_map) = self.entities.get(entity) {
            for (_, component_key) in component_key_map {
                output.push(*component_key);
            }
        } else {
            warn!("In WorldRecord.get_component_keys(), trying to access an entity that does not exist!");
        }

        output
    }

    pub fn get_key_from_type(&self, entity: &E, component_kind: &K) -> Option<ComponentKey> {
        if let Some(component_key_map) = self.entities.get(entity) {
            if let Some(component_key) = component_key_map.get(component_kind) {
                return Some(*component_key);
            }
        }
        return None;
    }

    pub fn get_component_record(&self, component_key: &ComponentKey) -> Option<(E, K)> {
        if let Some(record) = self.components.get(*component_key) {
            return Some(*record);
        }
        return None;
    }
}
