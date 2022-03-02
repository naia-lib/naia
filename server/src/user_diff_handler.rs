use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, RwLock, RwLockReadGuard},
    hash::Hash
};

use naia_shared::{DiffMask, ProtocolKindType};

use super::{global_diff_handler::GlobalDiffHandler, mut_channel::MutReceiver};

#[derive(Clone)]
pub struct UserDiffHandler<E: Copy + Eq + Hash, K: ProtocolKindType> {
    receivers: HashMap<(E, K), MutReceiver>,
    global_diff_handler: Arc<RwLock<GlobalDiffHandler<E, K>>>,
}

impl<E: Copy + Eq + Hash, K: ProtocolKindType> UserDiffHandler<E, K> {
    pub fn new(global_diff_handler: &Arc<RwLock<GlobalDiffHandler<E, K>>>) -> Self {
        UserDiffHandler {
            receivers: HashMap::new(),
            global_diff_handler: global_diff_handler.clone(),
        }
    }

    // Component Registration
    pub fn register_component(&mut self, addr: &SocketAddr, entity: &E, component_kind: &K) {
        if let Ok(global_handler) = self.global_diff_handler.as_ref().read() {
            let receiver = global_handler
                .receiver(addr, entity, component_kind)
                .expect("GlobalDiffHandler has not yet registered this Component");
            self.receivers.insert((*entity, *component_kind), receiver);
        }
    }

    pub fn deregister_component(&mut self, entity: &E, component_kind: &K) {
        self.receivers.remove(&(*entity, *component_kind));
    }

    // Diff masks
    pub fn diff_mask(&self, entity: &E, component_kind: &K) -> Option<RwLockReadGuard<DiffMask>> {
        if let Some(receiver) = self.receivers.get(&(*entity, *component_kind)) {
            return receiver.mask();
        }
        return None;
    }

    //    pub fn has_diff_mask(&self, component_key: &ComponentKey) -> bool {
    //        return self.receivers.contains_key(component_key);
    //    }

    pub fn diff_mask_is_clear(&self, entity: &E, component_kind: &K) -> bool {
        if let Some(receiver) = self.receivers.get(&(*entity, *component_kind)) {
            return receiver.diff_mask_is_clear();
        }
        return true;
    }

    pub fn or_diff_mask(&mut self, entity: &E, component_kind: &K, other_mask: &DiffMask) {
        if let Some(current_diff_mask) = self.receivers.get_mut(&(*entity, *component_kind)) {
            current_diff_mask.or_mask(other_mask);
        } else {
            // Either this, or the component is not registered somehow..
            warn!("attempting to retrieve a diff mask for a component which does not exist!");
        }
    }

    pub fn clear_diff_mask(&mut self, entity: &E, component_kind: &K) {
        if let Some(receiver) = self.receivers.get_mut(&(*entity, *component_kind)) {
            receiver.clear_mask();
        }
    }
}
