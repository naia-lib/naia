
use std::any::{TypeId};
use std::collections::HashMap;

use crate::{NetEvent, NetEventType, EventType};

pub struct EventManifest<T: EventType> {
    gaia_id_count: u16,
    gaia_id_map: HashMap<u16, T>,
    type_id_map: HashMap<TypeId, u16>,
}

impl<T: EventType> EventManifest<T> {
    pub fn new() -> Self {
        EventManifest {
            gaia_id_count: 0,
            gaia_id_map: HashMap::new(),
            type_id_map: HashMap::new()
        }
    }

    pub fn register_event<S: NetEvent<T>>(&mut self, some_type: &S) {
        let new_gaia_id = self.gaia_id_count;
        let type_id = NetEventType::get_type_id(some_type);
        self.type_id_map.insert(type_id, new_gaia_id);
        self.gaia_id_map.insert(new_gaia_id, NetEvent::<T>::to_type(some_type));
        self.gaia_id_count += 1;
    }

    pub fn get_gaia_id(&self, type_id: &TypeId) -> u16 {
        let gaia_id = self.type_id_map.get(type_id)
            .expect("hey I should get a TypeId here...");
        return *gaia_id;
    }

    pub fn create_event(&self, gaia_id: u16) -> Option<T> {
        match self.gaia_id_map.get(&gaia_id) {
            Some(event_type) => {
                return Some(event_type.clone());
            }
            None => {}
        }

        return None;
    }
}