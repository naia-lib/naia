use std::collections::HashMap;

use bevy::ecs::entity::Entity;

use naia_bevy_server::{RoomKey, UserKey};

pub struct Global {
    pub main_room_key: RoomKey,
    pub user_to_prediction_map: HashMap<UserKey, Entity>,
}
