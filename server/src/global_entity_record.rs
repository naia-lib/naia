use super::room::room_key::RoomKey;

pub struct GlobalEntityRecord {
    pub room_key: Option<RoomKey>,
}

impl GlobalEntityRecord {
    pub fn new() -> Self {
        Self { room_key: None }
    }
}
