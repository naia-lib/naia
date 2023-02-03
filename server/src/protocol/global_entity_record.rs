use std::collections::HashSet;

use naia_shared::{ComponentId, EntityHandle};

use crate::room::RoomKey;

pub struct GlobalEntityRecord {
    pub room_key: Option<RoomKey>,
    pub entity_handle: EntityHandle,
    pub component_kinds: HashSet<ComponentId>,
}

impl GlobalEntityRecord {
    pub fn new(entity_handle: EntityHandle) -> Self {
        Self {
            room_key: None,
            entity_handle,
            component_kinds: HashSet::new(),
        }
    }
}
