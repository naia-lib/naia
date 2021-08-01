use std::{collections::HashMap, net::SocketAddr};

use naia_shared::{Ref, DiffMask};

use super::object_key::object_key::ObjectKey;

use indexmap::IndexMap;

#[derive(Debug)]
pub struct MutHandler {
    state_diff_mask_list_map: HashMap<ObjectKey, IndexMap<SocketAddr, Ref<DiffMask>>>,
}

impl MutHandler {
    pub fn new() -> Ref<MutHandler> {
        Ref::new(MutHandler {
            state_diff_mask_list_map: HashMap::new(),
        })
    }

    pub fn mutate(&mut self, object_key: &ObjectKey, property_index: u8) {
        if let Some(diff_mask_list) = self.state_diff_mask_list_map.get_mut(object_key) {
            for (_, mask_ref) in diff_mask_list.iter_mut() {
                mask_ref.borrow_mut().set_bit(property_index, true);
            }
        }
    }

    pub fn clear_state(&mut self, address: &SocketAddr, object_key: &ObjectKey) {
        if let Some(diff_mask_list) = self.state_diff_mask_list_map.get_mut(object_key) {
            if let Some(mask_ref) = diff_mask_list.get(address) {
                mask_ref.borrow_mut().clear();
            }
        }
    }

    pub fn set_state(
        &mut self,
        address: &SocketAddr,
        object_key: &ObjectKey,
        other_state: &DiffMask,
    ) {
        if let Some(diff_mask_list) = self.state_diff_mask_list_map.get_mut(object_key) {
            if let Some(mask_ref) = diff_mask_list.get(address) {
                mask_ref.borrow_mut().copy_contents(other_state);
            }
        }
    }

    pub fn register_state(&mut self, object_key: &ObjectKey) {
        if self.state_diff_mask_list_map.contains_key(object_key) {
            panic!("State cannot register with server more than once!");
        }
        self.state_diff_mask_list_map
            .insert(*object_key, IndexMap::new());
    }

    pub fn deregister_state(&mut self, object_key: &ObjectKey) {
        self.state_diff_mask_list_map.remove(object_key);
    }

    pub fn register_mask(
        &mut self,
        address: &SocketAddr,
        object_key: &ObjectKey,
        mask: &Ref<DiffMask>,
    ) {
        if let Some(diff_mask_list) = self.state_diff_mask_list_map.get_mut(object_key) {
            diff_mask_list.insert(*address, mask.clone());
        }
    }

    pub fn deregister_mask(&mut self, address: &SocketAddr, object_key: &ObjectKey) {
        if let Some(diff_mask_list) = self.state_diff_mask_list_map.get_mut(object_key) {
            diff_mask_list.remove(address);
        }
    }
}
