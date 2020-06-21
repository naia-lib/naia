
use std::any::{TypeId};
use std::collections::HashMap;

use crate::{EventType, Entity, EntityTypeGetter, EntityType, EventBuilder};

pub struct Manifest<T: EventType, U: EntityType> {
    event_gaia_id_count: u16,
    event_builder_map: HashMap<u16, Box<dyn EventBuilder<T>>>,
    event_type_map: HashMap<TypeId, u16>,
    ////
    entity_gaia_id_count: u16,
    entity_gaia_id_map: HashMap<u16, U>,
    entity_type_id_map: HashMap<TypeId, u16>,
}

impl<T: EventType, U: EntityType> Manifest<T, U> {
    pub fn new() -> Self {
        Manifest {
            event_gaia_id_count: 0,
            event_builder_map: HashMap::new(),
            event_type_map: HashMap::new(),
            ///
            entity_gaia_id_count: 0,
            entity_gaia_id_map: HashMap::new(),
            entity_type_id_map: HashMap::new()
        }
    }

    pub fn register_event(&mut self, event_builder: Box<dyn EventBuilder<T>>) {
        let new_gaia_id = self.event_gaia_id_count;
        let type_id = event_builder.get_type_id();
        //let type_id = EventBuilderTypeGetter::<T, Event<T>>::get_type_id(&event_builder);//event_builder.get_type_id();
        self.event_type_map.insert(type_id, new_gaia_id);
        self.event_builder_map.insert(new_gaia_id, event_builder);
        self.event_gaia_id_count += 1;
    }

    pub fn get_event_gaia_id(&self, type_id: &TypeId) -> u16 {
        let gaia_id = self.event_type_map.get(type_id)
            .expect("hey I should get a TypeId here...");
        return *gaia_id;
    }

    pub fn create_event(&self, gaia_id: u16, bytes: &[u8]) -> Option<T> {
        match self.event_builder_map.get(&gaia_id) {
            Some(event_builder) => {
                return Some(event_builder.as_ref().build(bytes));
            }
            None => {}
        }

        return None;
    }

    pub fn register_entity<S: Entity<U>>(&mut self, some_type: &S) {
        let new_gaia_id = self.entity_gaia_id_count;
        let type_id = EntityTypeGetter::get_type_id(some_type);
        self.entity_type_id_map.insert(type_id, new_gaia_id);
        self.entity_gaia_id_map.insert(new_gaia_id, Entity::<U>::to_type(some_type));
        self.entity_gaia_id_count += 1;
    }

    pub fn get_entity_gaia_id(&self, type_id: &TypeId) -> u16 {
        let gaia_id = self.entity_type_id_map.get(type_id)
            .expect("hey I should get a TypeId here...");
        return *gaia_id;
    }

    pub fn create_entity(&self, gaia_id: u16) -> Option<U> {
        match self.entity_gaia_id_map.get(&gaia_id) {
            Some(entity_type) => {
                return Some(entity_type.init());
            }
            None => {}
        }

        return None;
    }
}