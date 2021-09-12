use std::any::TypeId;

use naia_shared::{ComponentRecord, Ref};

use super::{keys::component_key::ComponentKey, user::user_key::UserKey};

#[derive(Debug)]
pub struct EntityRecord {
    component_record: Ref<ComponentRecord<ComponentKey>>,
    owner: Option<UserKey>,
}

impl EntityRecord {
    pub fn new() -> Self {
        EntityRecord {
            component_record: Ref::new(ComponentRecord::new()),
            owner: None,
        }
    }

    pub fn set_owner(&mut self, user_key: &UserKey) {
        self.owner = Some(*user_key);
    }

    pub fn get_owner(&self) -> Option<&UserKey> {
        return self.owner.as_ref();
    }

    pub fn has_owner(&self) -> bool {
        return self.get_owner().is_some();
    }

    pub fn remove_owner(&mut self) {
        self.owner = None;
    }

    // Pass-through methods to underlying ComponentRecord
    pub fn get_component_record(&self) -> Ref<ComponentRecord<ComponentKey>> {
        return self.component_record.clone();
    }

    pub fn get_key_from_type(&self, type_id: &TypeId) -> Option<ComponentKey> {
        if let Some(key) = self.component_record.borrow().get_key_from_type(type_id) {
            return Some(*key);
        }
        return None;
    }

    pub fn get_component_keys(&self) -> Vec<ComponentKey> {
        return self.component_record.borrow().get_component_keys();
    }

    pub fn insert_component(&self, key: &ComponentKey, type_id: &TypeId) {
        return self
            .component_record
            .borrow_mut()
            .insert_component(key, type_id);
    }

    pub fn remove_component(&self, key: &ComponentKey) {
        return self.component_record.borrow_mut().remove_component(key);
    }
}
