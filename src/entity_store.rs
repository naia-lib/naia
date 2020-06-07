use std::{
    rc::Rc,
    cell::RefCell,
};

use crate::{EntityType, NetEntity};

use slotmap::{new_key_type, DenseSlotMap};

new_key_type! { pub struct EntityKey; }

pub struct EntityStore<T: EntityType> {
    map: DenseSlotMap<EntityKey, Rc<RefCell<dyn NetEntity<T>>>>,
}

impl<T: EntityType> EntityStore<T> {
    pub fn new() -> EntityStore<T> {
        EntityStore {
            map: DenseSlotMap::with_key(),
        }
    }

    pub fn add_entity(&mut self, entity: Rc<RefCell<dyn NetEntity<T>>>) -> EntityKey {
        return self.map.insert(entity);
    }
}