use std::{
    collections::{HashSet, HashMap},
    rc::Rc,
    cell::RefCell,
    net::SocketAddr,
};

use crate::{EntityKey, StateMask};

use indexmap::IndexMap;

pub struct MutHandler {
    entity_state_mask_list_map: HashMap<EntityKey, IndexMap<SocketAddr, Rc<RefCell<StateMask>>>>,
}

impl MutHandler {
    pub fn new() -> Rc<RefCell<MutHandler>> {
        Rc::new(RefCell::new(MutHandler {
            entity_state_mask_list_map: HashMap::new(),
        }))
    }

    pub fn mutate(&mut self, entity_key: &EntityKey, property_index: u8) {
        if let Some(state_mask_list) = self.entity_state_mask_list_map.get_mut(entity_key) {
            for (_, mask_ref) in state_mask_list.iter_mut() {
                mask_ref.borrow_mut().set_bit(property_index, true);
            }
        }
    }

    pub fn clear_state(&mut self, address: &SocketAddr, entity_key: &EntityKey) {
        if let Some(state_mask_list) = self.entity_state_mask_list_map.get_mut(entity_key) {
            if let Some(mask_ref) = state_mask_list.get(address) {
                mask_ref.borrow_mut().clear();
            }
        }
    }

    pub fn register_mask(&mut self, address: &SocketAddr, entity_key: &EntityKey, mask: &Rc<RefCell<StateMask>>) {
        if !self.entity_state_mask_list_map.contains_key(entity_key) {
            self.entity_state_mask_list_map.insert(*entity_key, IndexMap::new());
        }

        if let Some(state_mask_list) = self.entity_state_mask_list_map.get_mut(entity_key) {
            state_mask_list.insert(*address, mask.clone());
        }
    }

    pub fn deregister_mask(&mut self, address: &SocketAddr, entity_key: &EntityKey) {
        if let Some(state_mask_list) = self.entity_state_mask_list_map.get_mut(entity_key) {
            state_mask_list.remove(address);
        }
    }
}


// we want:
// 1. an entity to immediately get a mutable list of all records out there associated with it, then iterate: Vec
// 2. be able to add new records to an entity's list when it comes into scope for a given connection
// 3. be able to remove records from an entities list when it goes out of scope for a given connection