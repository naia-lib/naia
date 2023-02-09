use std::any::TypeId;
use std::collections::HashSet;

use naia_shared::{ComponentKind, EntityHandle};

use crate::room::RoomKey;

pub struct GlobalEntityRecord {
    pub room_key: Option<RoomKey>,
    pub entity_handle: EntityHandle,
    pub component_kinds: HashSet<TypeId>,
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
