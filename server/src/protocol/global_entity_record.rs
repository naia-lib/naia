use std::collections::HashSet;

use naia_shared::{EntityHandle, ProtocolKindType};

use crate::room::RoomKey;

pub struct GlobalEntityRecord<K: ProtocolKindType> {
    pub room_key: Option<RoomKey>,
    pub entity_handle: EntityHandle,
    pub component_kinds: HashSet<K>,
}

impl<K: ProtocolKindType> GlobalEntityRecord<K> {
    pub fn new(entity_handle: EntityHandle) -> Self {
        Self {
            room_key: None,
            entity_handle,
            component_kinds: HashSet::new(),
        }
    }
}
