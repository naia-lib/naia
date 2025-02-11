use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, RwLock, RwLockReadGuard},
};

use crate::{ComponentKind, DiffMask, GlobalEntity, GlobalWorldManagerType};

use super::{global_diff_handler::GlobalDiffHandler, mut_channel::MutReceiver};

#[derive(Clone)]
pub struct UserDiffHandler {
    receivers: HashMap<(GlobalEntity, ComponentKind), MutReceiver>,
    global_diff_handler: Arc<RwLock<GlobalDiffHandler>>,
}

impl UserDiffHandler {
    pub fn new(global_world_manager: &dyn GlobalWorldManagerType) -> Self {
        Self {
            receivers: HashMap::new(),
            global_diff_handler: global_world_manager.diff_handler(),
        }
    }

    // Component Registration
    pub fn register_component(
        &mut self,
        address: &Option<SocketAddr>,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) {
        let Ok(global_handler) = self.global_diff_handler.as_ref().read() else {
            panic!("Be sure you can get self.global_diff_handler before calling this!");
        };
        let receiver = global_handler
            .receiver(address, entity, component_kind)
            .expect("GlobalDiffHandler has not yet registered this Component");
        self.receivers.insert((*entity, *component_kind), receiver);
    }

    pub fn deregister_component(&mut self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        self.receivers.remove(&(*entity, *component_kind));
    }

    pub fn has_component(&self, entity: &GlobalEntity, component: &ComponentKind) -> bool {
        self.receivers.contains_key(&(*entity, *component))
    }

    // Diff masks
    pub fn diff_mask(
        &self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) -> RwLockReadGuard<DiffMask> {
        let Some(receiver) = self.receivers.get(&(*entity, *component_kind)) else {
            panic!("Should not call this unless we're sure there's a receiver");
        };
        return receiver.mask();
    }

    //    pub fn has_diff_mask(&self, component_key: &ComponentKey) -> bool {
    //        return self.receivers.contains_key(component_key);
    //    }

    pub fn diff_mask_is_clear(&self, entity: &GlobalEntity, component_kind: &ComponentKind) -> bool {
        let Some(receiver) = self.receivers.get(&(*entity, *component_kind)) else {
            panic!("Should not call this unless we're sure there's a receiver");
        };
        return receiver.diff_mask_is_clear();
    }

    pub fn or_diff_mask(
        &mut self,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
        other_mask: &DiffMask,
    ) {
        let Some(receiver) = self.receivers.get_mut(&(*entity, *component_kind)) else {
            panic!("Should not call this unless we're sure there's a receiver");
        };
        receiver.or_mask(other_mask);
    }

    pub fn clear_diff_mask(&mut self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        let Some(receiver) = self.receivers.get_mut(&(*entity, *component_kind)) else {
            panic!("Should not call this unless we're sure there's a receiver");
        };
        receiver.clear_mask();
    }
}
