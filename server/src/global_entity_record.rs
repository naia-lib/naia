use super::{user::user_key::UserKey, room::room_key::RoomKey};

pub struct GlobalEntityRecord {
    pub owner_key: Option<UserKey>,
    pub room_key: Option<RoomKey>,
}

impl GlobalEntityRecord {
    pub fn new() -> Self {
        Self {
            owner_key: None,
            room_key: None
        }
    }
}