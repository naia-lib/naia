use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
};

pub use naia_shared::Ref;

use crate::ComponentKey;

pub struct EntityComponentRecord {
    key_set_ref: Ref<HashSet<ComponentKey>>,
    type_map: HashMap<TypeId, ComponentKey>,
}

impl EntityComponentRecord {
    pub fn new() -> Self {
        EntityComponentRecord {
            key_set_ref: Ref::new(HashSet::new()),
            type_map: HashMap::new(),
        }
    }

    pub fn get_component_set(&self) -> &Ref<HashSet<ComponentKey>> {
        return &self.key_set_ref;
    }

    pub fn get_key_from_type(&self, type_id: &TypeId) -> Option<&ComponentKey> {
        return self.type_map.get(type_id);
    }
}