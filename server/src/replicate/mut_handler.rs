use std::{collections::HashMap, net::SocketAddr};

use naia_shared::{Ref, DiffMask};

use super::object_key::object_key::ObjectKey;

use indexmap::IndexMap;

#[derive(Debug)]
pub struct MutHandler {
    replicate_diff_mask_list_map: HashMap<ObjectKey, IndexMap<SocketAddr, Ref<DiffMask>>>,
}

impl MutHandler {
    pub fn new() -> Ref<MutHandler> {
        Ref::new(MutHandler {
            replicate_diff_mask_list_map: HashMap::new(),
        })
    }

    pub fn mutate(&mut self, object_key: &ObjectKey, property_index: u8) {
        if let Some(diff_mask_list) = self.replicate_diff_mask_list_map.get_mut(object_key) {
            for (_, mask_ref) in diff_mask_list.iter_mut() {
                mask_ref.borrow_mut().set_bit(property_index, true);
            }
        }
    }

    pub fn clear_replicate(&mut self, address: &SocketAddr, object_key: &ObjectKey) {
        if let Some(diff_mask_list) = self.replicate_diff_mask_list_map.get_mut(object_key) {
            if let Some(mask_ref) = diff_mask_list.get(address) {
                mask_ref.borrow_mut().clear();
            }
        }
    }

    pub fn set_replicate(
        &mut self,
        address: &SocketAddr,
        object_key: &ObjectKey,
        other_replicate: &DiffMask,
    ) {
        if let Some(diff_mask_list) = self.replicate_diff_mask_list_map.get_mut(object_key) {
            if let Some(mask_ref) = diff_mask_list.get(address) {
                mask_ref.borrow_mut().copy_contents(other_replicate);
            }
        }
    }

    pub fn register_replicate(&mut self, object_key: &ObjectKey) {
        if self.replicate_diff_mask_list_map.contains_key(object_key) {
            panic!("Replicate cannot register with server more than once!");
        }
        self.replicate_diff_mask_list_map
            .insert(*object_key, IndexMap::new());
    }

    pub fn deregister_replicate(&mut self, object_key: &ObjectKey) {
        self.replicate_diff_mask_list_map.remove(object_key);
    }

    pub fn register_mask(
        &mut self,
        address: &SocketAddr,
        object_key: &ObjectKey,
        mask: &Ref<DiffMask>,
    ) {
        if let Some(diff_mask_list) = self.replicate_diff_mask_list_map.get_mut(object_key) {
            diff_mask_list.insert(*address, mask.clone());
        }
    }

    pub fn deregister_mask(&mut self, address: &SocketAddr, object_key: &ObjectKey) {
        if let Some(diff_mask_list) = self.replicate_diff_mask_list_map.get_mut(object_key) {
            diff_mask_list.remove(address);
        }
    }
}
