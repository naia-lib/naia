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

    pub fn set_state(&mut self, address: &SocketAddr, entity_key: &EntityKey, other_state: &StateMask) {
        if let Some(state_mask_list) = self.entity_state_mask_list_map.get_mut(entity_key) {
            if let Some(mask_ref) = state_mask_list.get(address) {
                mask_ref.borrow_mut().copy_contents(other_state);
            }
        }
    }

    pub fn register_entity(&mut self, entity_key: &EntityKey) {
        self.entity_state_mask_list_map.insert(*entity_key, IndexMap::new());
    }

    pub fn deregister_entity(&mut self, entity_key: &EntityKey) {
        self.entity_state_mask_list_map.remove(entity_key);
    }

    pub fn register_mask(&mut self, address: &SocketAddr, entity_key: &EntityKey, mask: &Rc<RefCell<StateMask>>) {
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