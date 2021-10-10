use std::collections::HashMap;

use naia_bevy_server::{RoomKey, UserKey, Entity};

pub struct Global {
    pub main_room_key: RoomKey,
    pub user_to_prediction_map: HashMap<UserKey, Entity>,
}
