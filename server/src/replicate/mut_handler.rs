use std::{collections::HashMap, net::SocketAddr};

use naia_shared::{DiffMask, Ref};

use super::keys::replicate_key::ReplicateKey;

use indexmap::IndexMap;

#[derive(Debug)]
pub struct MutHandler {
    replicate_diff_mask_list_map: HashMap<ReplicateKey, IndexMap<SocketAddr, Ref<DiffMask>>>,
}

impl MutHandler {
    pub fn new() -> Ref<MutHandler> {
        Ref::new(MutHandler {
            replicate_diff_mask_list_map: HashMap::new(),
        })
    }

    pub fn mutate(&mut self, replicate_key: &ReplicateKey, property_index: u8) {
        if let Some(diff_mask_list) = self.replicate_diff_mask_list_map.get_mut(replicate_key) {
            for (_, mask_ref) in diff_mask_list.iter_mut() {
                mask_ref.borrow_mut().set_bit(property_index, true);
            }
        }
    }

    pub fn clear_replicate(&mut self, address: &SocketAddr, replicate_key: &ReplicateKey) {
        if let Some(diff_mask_list) = self.replicate_diff_mask_list_map.get_mut(replicate_key) {
            if let Some(mask_ref) = diff_mask_list.get(address) {
                mask_ref.borrow_mut().clear();
            }
        }
    }

    pub fn set_replicate(
        &mut self,
        address: &SocketAddr,
        replicate_key: &ReplicateKey,
        other_replicate: &DiffMask,
    ) {
        if let Some(diff_mask_list) = self.replicate_diff_mask_list_map.get_mut(replicate_key) {
            if let Some(mask_ref) = diff_mask_list.get(address) {
                mask_ref.borrow_mut().copy_contents(other_replicate);
            }
        }
    }

    pub fn register_replicate(&mut self, replicate_key: &ReplicateKey) {
        if self
            .replicate_diff_mask_list_map
            .contains_key(replicate_key)
        {
            panic!("Replicate cannot register with server more than once!");
        }
        self.replicate_diff_mask_list_map
            .insert(*replicate_key, IndexMap::new());
    }

    pub fn deregister_replicate(&mut self, replicate_key: &ReplicateKey) {
        self.replicate_diff_mask_list_map.remove(replicate_key);
    }

    pub fn register_mask(
        &mut self,
        address: &SocketAddr,
        replicate_key: &ReplicateKey,
        mask: &Ref<DiffMask>,
    ) {
        if let Some(diff_mask_list) = self.replicate_diff_mask_list_map.get_mut(replicate_key) {
            diff_mask_list.insert(*address, mask.clone());
        }
    }

    pub fn deregister_mask(&mut self, address: &SocketAddr, replicate_key: &ReplicateKey) {
        if let Some(diff_mask_list) = self.replicate_diff_mask_list_map.get_mut(replicate_key) {
            diff_mask_list.remove(address);
        }
    }
}
