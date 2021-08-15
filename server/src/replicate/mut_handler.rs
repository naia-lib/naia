use std::{collections::HashMap, net::SocketAddr};

use naia_shared::{DiffMask, Ref};

use super::keys::replica_key::ReplicaKey;

use indexmap::IndexMap;

#[derive(Debug)]
pub struct MutHandler {
    replica_diff_mask_list_map: HashMap<ReplicaKey, IndexMap<SocketAddr, Ref<DiffMask>>>,
}

impl MutHandler {
    pub fn new() -> Ref<MutHandler> {
        Ref::new(MutHandler {
            replica_diff_mask_list_map: HashMap::new(),
        })
    }

    pub fn mutate(&mut self, replica_key: &ReplicaKey, property_index: u8) {
        if let Some(diff_mask_list) = self.replica_diff_mask_list_map.get_mut(replica_key) {
            for (_, mask_ref) in diff_mask_list.iter_mut() {
                mask_ref.borrow_mut().set_bit(property_index, true);
            }
        }
    }

    pub fn clear_replica(&mut self, address: &SocketAddr, replica_key: &ReplicaKey) {
        if let Some(diff_mask_list) = self.replica_diff_mask_list_map.get_mut(replica_key) {
            if let Some(mask_ref) = diff_mask_list.get(address) {
                mask_ref.borrow_mut().clear();
            }
        }
    }

    pub fn set_replica(
        &mut self,
        address: &SocketAddr,
        replica_key: &ReplicaKey,
        other_replica: &DiffMask,
    ) {
        if let Some(diff_mask_list) = self.replica_diff_mask_list_map.get_mut(replica_key) {
            if let Some(mask_ref) = diff_mask_list.get(address) {
                mask_ref.borrow_mut().copy_contents(other_replica);
            }
        }
    }

    pub fn register_replica(&mut self, replica_key: &ReplicaKey) {
        if self
            .replica_diff_mask_list_map
            .contains_key(replica_key)
        {
            panic!("Replica cannot register with server more than once!");
        }
        self.replica_diff_mask_list_map
            .insert(*replica_key, IndexMap::new());
    }

    pub fn deregister_replica(&mut self, replica_key: &ReplicaKey) {
        self.replica_diff_mask_list_map.remove(replica_key);
    }

    pub fn register_mask(
        &mut self,
        address: &SocketAddr,
        replica_key: &ReplicaKey,
        mask: &Ref<DiffMask>,
    ) {
        if let Some(diff_mask_list) = self.replica_diff_mask_list_map.get_mut(replica_key) {
            diff_mask_list.insert(*address, mask.clone());
        }
    }

    pub fn deregister_mask(&mut self, address: &SocketAddr, replica_key: &ReplicaKey) {
        if let Some(diff_mask_list) = self.replica_diff_mask_list_map.get_mut(replica_key) {
            diff_mask_list.remove(address);
        }
    }
}
