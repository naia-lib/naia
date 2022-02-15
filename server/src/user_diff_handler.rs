use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, RwLock, RwLockReadGuard},
};

use naia_shared::DiffMask;

use super::{global_diff_handler::GlobalDiffHandler, keys::ComponentKey, mut_channel::MutReceiver};

#[derive(Clone)]
pub struct UserDiffHandler {
    receivers: HashMap<ComponentKey, MutReceiver>,
    global_diff_handler: Arc<RwLock<GlobalDiffHandler>>,
}

impl UserDiffHandler {
    pub fn new(global_diff_handler: &Arc<RwLock<GlobalDiffHandler>>) -> Self {
        UserDiffHandler {
            receivers: HashMap::new(),
            global_diff_handler: global_diff_handler.clone(),
        }
    }

    // Component Registration
    pub fn register_component(&mut self, addr: &SocketAddr, component_key: &ComponentKey) {
        if let Ok(global_handler) = self.global_diff_handler.as_ref().read() {
            let receiver = global_handler
                .receiver(addr, component_key)
                .expect("GlobalDiffHandler has not yet registered this Component");
            self.receivers.insert(*component_key, receiver);
        }
    }

    pub fn deregister_component(&mut self, component_key: &ComponentKey) {
        self.receivers.remove(component_key);
    }

    // Diff masks
    pub fn diff_mask(&self, component_key: &ComponentKey) -> Option<RwLockReadGuard<DiffMask>> {
        if let Some(receiver) = self.receivers.get(component_key) {
            return receiver.mask();
        }
        return None;
    }

    //    pub fn has_diff_mask(&self, component_key: &ComponentKey) -> bool {
    //        return self.receivers.contains_key(component_key);
    //    }

    pub fn diff_mask_is_clear(&self, component_key: &ComponentKey) -> bool {
        if let Some(receiver) = self.receivers.get(component_key) {
            return receiver.diff_mask_is_clear();
        }
        return true;
    }

    pub fn or_diff_mask(&mut self, component_key: &ComponentKey, other_mask: &DiffMask) {
        if let Some(current_diff_mask) = self.receivers.get_mut(component_key) {
            current_diff_mask.or_mask(other_mask);
        } else {
            // Either this, or the component is not registered somehow..
            warn!("attempting to retrieve a diff mask for a component which does not exist!");
        }
    }

    pub fn clear_diff_mask(&mut self, component_key: &ComponentKey) {
        if let Some(receiver) = self.receivers.get_mut(component_key) {
            receiver.clear_mask();
        }
    }

    pub fn set_diff_mask(&mut self, component_key: &ComponentKey, other_mask: &DiffMask) {
        if let Some(receiver) = self.receivers.get_mut(component_key) {
            receiver.set_mask(other_mask);
        }
    }
}
