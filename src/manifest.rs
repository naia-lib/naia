use std::any::TypeId;
use std::collections::HashMap;

use crate::{EntityBuilder, EntityType, EventBuilder, EventType};

pub struct Manifest<T: EventType, U: EntityType> {
    event_naia_id_count: u16,
    event_builder_map: HashMap<u16, Box<dyn EventBuilder<T>>>,
    event_type_map: HashMap<TypeId, u16>,
    ////
    entity_naia_id_count: u16,
    entity_builder_map: HashMap<u16, Box<dyn EntityBuilder<U>>>,
    entity_type_map: HashMap<TypeId, u16>,
}

impl<T: EventType, U: EntityType> Manifest<T, U> {
    pub fn new() -> Self {
        Manifest {
            event_naia_id_count: 0,
            event_builder_map: HashMap::new(),
            event_type_map: HashMap::new(),
            ///
            entity_naia_id_count: 0,
            entity_builder_map: HashMap::new(),
            entity_type_map: HashMap::new(),
        }
    }

    pub fn register_event(&mut self, event_builder: Box<dyn EventBuilder<T>>) {
        let new_naia_id = self.event_naia_id_count;
        let type_id = event_builder.get_type_id();
        self.event_type_map.insert(type_id, new_naia_id);
        self.event_builder_map.insert(new_naia_id, event_builder);
        self.event_naia_id_count += 1;
    }

    pub fn get_event_naia_id(&self, type_id: &TypeId) -> u16 {
        let naia_id = self
            .event_type_map
            .get(type_id)
            .expect("hey I should get a TypeId here...");
        return *naia_id;
    }

    pub fn create_event(&self, naia_id: u16, bytes: &[u8]) -> Option<T> {
        match self.event_builder_map.get(&naia_id) {
            Some(event_builder) => {
                return Some(event_builder.as_ref().build(bytes));
            }
            None => {}
        }

        return None;
    }

    pub fn register_entity(&mut self, entity_builder: Box<dyn EntityBuilder<U>>) {
        let new_naia_id = self.entity_naia_id_count;
        let type_id = entity_builder.get_type_id();
        self.entity_type_map.insert(type_id, new_naia_id);
        self.entity_builder_map.insert(new_naia_id, entity_builder);
        self.entity_naia_id_count += 1;
    }

    pub fn get_entity_naia_id(&self, type_id: &TypeId) -> u16 {
        let naia_id = self
            .entity_type_map
            .get(type_id)
            .expect("hey I should get a TypeId here...");
        return *naia_id;
    }

    pub fn create_entity(&self, naia_id: u16, bytes: &[u8]) -> Option<U> {
        match self.entity_builder_map.get(&naia_id) {
            Some(entity_builder) => {
                return Some(entity_builder.as_ref().build(bytes));
            }
            None => {}
        }

        return None;
    }
}
