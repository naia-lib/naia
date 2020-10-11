use std::{cell::RefCell, collections::HashMap, net::SocketAddr, rc::Rc};

use naia_shared::StateMask;

use crate::actors::actor_key::actor_key::ActorKey;

use indexmap::IndexMap;

#[derive(Debug)]
pub struct MutHandler {
    actor_state_mask_list_map: HashMap<ActorKey, IndexMap<SocketAddr, Rc<RefCell<StateMask>>>>,
}

impl MutHandler {
    pub fn new() -> Rc<RefCell<MutHandler>> {
        Rc::new(RefCell::new(MutHandler {
            actor_state_mask_list_map: HashMap::new(),
        }))
    }

    pub fn mutate(&mut self, actor_key: &ActorKey, property_index: u8) {
        if let Some(state_mask_list) = self.actor_state_mask_list_map.get_mut(actor_key) {
            for (_, mask_ref) in state_mask_list.iter_mut() {
                mask_ref.borrow_mut().set_bit(property_index, true);
            }
        }
    }

    pub fn clear_state(&mut self, address: &SocketAddr, actor_key: &ActorKey) {
        if let Some(state_mask_list) = self.actor_state_mask_list_map.get_mut(actor_key) {
            if let Some(mask_ref) = state_mask_list.get(address) {
                mask_ref.borrow_mut().clear();
            }
        }
    }

    pub fn set_state(
        &mut self,
        address: &SocketAddr,
        actor_key: &ActorKey,
        other_state: &StateMask,
    ) {
        if let Some(state_mask_list) = self.actor_state_mask_list_map.get_mut(actor_key) {
            if let Some(mask_ref) = state_mask_list.get(address) {
                mask_ref.borrow_mut().copy_contents(other_state);
            }
        }
    }

    pub fn register_actor(&mut self, actor_key: &ActorKey) {
        if self.actor_state_mask_list_map.contains_key(actor_key) {
            panic!("Actor cannot register with server more than once!");
        }
        self.actor_state_mask_list_map
            .insert(*actor_key, IndexMap::new());
    }

    pub fn deregister_actor(&mut self, actor_key: &ActorKey) {
        self.actor_state_mask_list_map.remove(actor_key);
    }

    pub fn register_mask(
        &mut self,
        address: &SocketAddr,
        actor_key: &ActorKey,
        mask: &Rc<RefCell<StateMask>>,
    ) {
        if let Some(state_mask_list) = self.actor_state_mask_list_map.get_mut(actor_key) {
            state_mask_list.insert(*address, mask.clone());
        }
    }

    pub fn deregister_mask(&mut self, address: &SocketAddr, actor_key: &ActorKey) {
        if let Some(state_mask_list) = self.actor_state_mask_list_map.get_mut(actor_key) {
            state_mask_list.remove(address);
        }
    }
}
