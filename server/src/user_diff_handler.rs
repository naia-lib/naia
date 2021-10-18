use std::{net::SocketAddr, collections::HashMap, sync::{Arc, RwLock}};


use naia_shared::DiffMask;

use super::{global_diff_handler::GlobalDiffHandler, mutcaster::MutReceiver, keys::ComponentKey};

#[derive(Clone)]
pub struct UserDiffHandler {
    components: HashMap<ComponentKey, (MutReceiver, DiffMask)>,
    global_diff_handler: Arc<RwLock<GlobalDiffHandler>>
}

impl UserDiffHandler {
    pub fn new(global_diff_handler: &Arc<RwLock<GlobalDiffHandler>>) -> Self {
        UserDiffHandler {
            global_diff_handler: global_diff_handler.clone(),
            components: HashMap::new(),
        }
    }

    // For Connection (diff masks)
    pub fn get_diff_mask(&self, component_key: &ComponentKey) -> Option<&DiffMask> {
        if let Some((_, diff_mask)) = self.components.get(component_key) {
            return Some(diff_mask);
        }
        return None;
    }

    pub fn has_diff(&self, component_key: &ComponentKey) -> bool {
        if let Some((_, diff_mask)) = self.components.get(component_key) {
            return !diff_mask.is_clear();
        }
        return false;
    }

    pub fn diff_mask_or(&mut self, component_key: &ComponentKey, new_diff_mask: &DiffMask) {
        let (_, current_diff_mask) = self.components.get_mut(component_key).expect("DiffHandler doesn't have Component registered");
        current_diff_mask.or(new_diff_mask);
    }

    pub fn clear_component(&mut self, component_key: &ComponentKey) {
        if let Some(record) = self.components.get_mut(component_key) {
            record.1.clear();
        }
    }

    pub fn set_component(
        &mut self,
        component_key: &ComponentKey,
        other_component: &DiffMask,
    ) {
        if let Some(record) = self.components.get_mut(component_key) {
            record.1.copy_contents(other_component);
        }
    }

    pub fn register_component(
        &mut self,
        addr: &SocketAddr,
        component_key: &ComponentKey,
        diff_mask_size: u8)
    {
        if let Ok(global_handler) = self.global_diff_handler.as_ref().read() {
            let receiver = global_handler.get_receiver(component_key, addr).expect("GlobalDiffHandler has not yet registered this Component");
            self.components.insert(*component_key,
                                   (receiver,
                                    DiffMask::new(diff_mask_size)));
        }
    }

    pub fn deregister_component(&mut self, component_key: &ComponentKey) {
        self.components.remove(component_key);
    }
}
