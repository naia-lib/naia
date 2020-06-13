use std::{
    rc::Rc,
    cell::RefCell,
};

use gaia_shared::{EntityType, Entity};

use super::{
    entity_key::EntityKey,
};

use slotmap::{DenseSlotMap};

pub struct EntityStore<T: EntityType> {
    map: DenseSlotMap<EntityKey, Rc<RefCell<dyn Entity<T>>>>,
}

impl<T: EntityType> EntityStore<T> {
    pub fn new() -> EntityStore<T> {
        EntityStore {
            map: DenseSlotMap::with_key(),
        }
    }

    pub fn add_entity(&mut self, entity: Rc<RefCell<dyn Entity<T>>>) -> EntityKey {
        return self.map.insert(entity);
    }

    pub fn remove_entity(&mut self, key: EntityKey) {
        self.map.remove(key);
    }

    pub fn get_entity(&mut self, key: EntityKey) -> Option<&Rc<RefCell<dyn Entity<T>>>> {
        return self.map.get(key);
    }

    pub fn has_entity(&self, key: EntityKey) -> bool {
        return self.map.contains_key(key);
    }

    pub fn iter(&self) -> slotmap::dense::Iter<EntityKey, Rc<RefCell<dyn Entity<T>>>> {
        return self.map.iter();
    }
}