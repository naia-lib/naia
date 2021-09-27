use std::{collections::HashMap, net::SocketAddr};

use naia_shared::{DiffMask, Ref};

use super::keys::{ComponentKey, KeyType};

use indexmap::IndexMap;

#[derive(Debug)]
pub struct MutHandler<K: KeyType> {
    component_diff_mask_list_map: HashMap<ComponentKey<K>, IndexMap<SocketAddr, Ref<DiffMask>>>,
}

impl<K: KeyType> MutHandler<K> {
    pub fn new() -> Ref<MutHandler<K>> {
        Ref::new(MutHandler {
            component_diff_mask_list_map: HashMap::new(),
        })
    }

    pub fn mutate(&mut self, component_key: &ComponentKey<K>, property_index: u8) {
        if let Some(diff_mask_list) = self.component_diff_mask_list_map.get_mut(component_key) {
            for (_, mask_ref) in diff_mask_list.iter_mut() {
                mask_ref.borrow_mut().set_bit(property_index, true);
            }
        }
    }

    pub fn clear_component(&mut self, address: &SocketAddr, component_key: &ComponentKey<K>) {
        if let Some(diff_mask_list) = self.component_diff_mask_list_map.get_mut(component_key) {
            if let Some(mask_ref) = diff_mask_list.get(address) {
                mask_ref.borrow_mut().clear();
            }
        }
    }

    pub fn set_component(
        &mut self,
        address: &SocketAddr,
        component_key: &ComponentKey<K>,
        other_component: &DiffMask,
    ) {
        if let Some(diff_mask_list) = self.component_diff_mask_list_map.get_mut(component_key) {
            if let Some(mask_ref) = diff_mask_list.get(address) {
                mask_ref.borrow_mut().copy_contents(other_component);
            }
        }
    }

    pub fn register_component(&mut self, component_key: &ComponentKey<K>) {
        if self
            .component_diff_mask_list_map
            .contains_key(component_key)
        {
            panic!("Component cannot register with server more than once!");
        }

        self.component_diff_mask_list_map
            .insert(*component_key, IndexMap::new());
    }

    pub fn deregister_component(&mut self, component_key: &ComponentKey<K>) {
        self.component_diff_mask_list_map.remove(component_key);
    }

    pub fn register_mask(
        &mut self,
        address: &SocketAddr,
        component_key: &ComponentKey<K>,
        mask: &Ref<DiffMask>,
    ) {
        if let Some(diff_mask_list) = self.component_diff_mask_list_map.get_mut(component_key) {
            diff_mask_list.insert(*address, mask.clone());
        }
    }

    pub fn deregister_mask(&mut self, address: &SocketAddr, component_key: &ComponentKey<K>) {
        if let Some(diff_mask_list) = self.component_diff_mask_list_map.get_mut(component_key) {
            diff_mask_list.remove(address);
        }
    }
}
