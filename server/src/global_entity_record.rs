use std::collections::HashSet;

use naia_shared::{EntityHandleInner, ProtocolKindType};

use super::room::RoomKey;

pub struct GlobalEntityRecord<K: ProtocolKindType> {
    pub room_key: Option<RoomKey>,
    pub entity_handle: Option<EntityHandleInner>,
    pub component_kinds: HashSet<K>,
}

impl<K: ProtocolKindType> GlobalEntityRecord<K> {
    pub fn new() -> Self {
        Self {
            room_key: None,
            entity_handle: None,
            component_kinds: HashSet::new(),
        }
    }
}
