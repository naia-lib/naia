use std::{collections::HashMap, hash::Hash};
use std::collections::HashSet;

use naia_shared::ProtocolKindType;

pub struct WorldRecord<E: Copy + Eq + Hash, K: ProtocolKindType> {
    entities: HashMap<E, HashSet<K>>,
}

impl<E: Copy + Eq + Hash, K: ProtocolKindType> WorldRecord<E, K> {
    pub fn new() -> Self {
        WorldRecord {
            entities: HashMap::new(),
        }
    }

    // Sync w/ World & Server

    pub fn spawn_entity(&mut self, entity: &E) {
        if self.entities.contains_key(entity) {
            panic!("entity already initialized!");
        }
        self.entities.insert(*entity, HashSet::new());
    }

    pub fn despawn_entity(&mut self, entity: &E) {
        if !self.entities.contains_key(entity) {
            panic!("entity does not exist!");
        }

        self.entities.remove(entity);
    }

    pub fn add_component(&mut self, entity: &E, component_type: &K) {
        if !self.entities.contains_key(entity) {
            panic!("entity does not exist!");
        }
        let component_kind_set = self.entities.get_mut(entity).unwrap();
        component_kind_set.insert(*component_type);
    }

    pub fn remove_component(&mut self, entity: &E, component_kind: &K) {
        if let Some(component_kind_set) = self.entities.get_mut(entity) {
            if !component_kind_set.remove(component_kind) {
                panic!("component does not exist!");
            }
        } else {
            panic!("entity does not exist!");
        }
    }

    // Access

    pub fn has_entity(&self, entity: &E) -> bool {
        return self.entities.contains_key(entity);
    }

    pub fn component_kinds(&self, entity: &E) -> Vec<K> {
        let mut output = Vec::new();

        if let Some(component_kind_set) = self.entities.get(entity) {
            for component_kind in component_kind_set {
                output.push(*component_kind);
            }
        } else {
            warn!(
                "In WorldRecord.component_keys(), trying to access an entity that does not exist!"
            );
        }

        output
    }
}
