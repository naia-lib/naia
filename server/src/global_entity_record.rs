use naia_shared::EntityHandleInner;

use super::room::RoomKey;

pub struct GlobalEntityRecord {
    pub room_key: Option<RoomKey>,
    pub entity_handle: Option<EntityHandleInner>
}

impl GlobalEntityRecord {
    pub fn new() -> Self {
        Self { room_key: None, entity_handle: None }
    }
}
