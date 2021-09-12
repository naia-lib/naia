use std::any::TypeId;

use naia_shared::{ComponentRecord, LocalComponentKey};

#[derive(Debug)]
pub struct EntityRecord {
    component_record: ComponentRecord<LocalComponentKey>,
    pub is_pawn: bool,
}

impl EntityRecord {
    pub fn new() -> Self {
        EntityRecord {
            component_record: ComponentRecord::new(),
            is_pawn: false,
        }
    }

    pub fn get_key_from_type(&self, type_id: &TypeId) -> Option<&LocalComponentKey> {
        return self.component_record.get_key_from_type(type_id);
    }

    pub fn insert_component(&mut self, key: &LocalComponentKey, type_id: &TypeId) {
        self.component_record.insert_component(key, type_id);
    }

    pub fn remove_component(&mut self, key: &LocalComponentKey) {
        self.component_record.remove_component(key);
    }

    pub fn get_component_keys(&self) -> Vec<LocalComponentKey> {
        return self.component_record.get_component_keys();
    }
}
