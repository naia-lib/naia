use std::collections::HashMap;

use bevy_ecs::{entity::Entity, prelude::Resource};

use naia_bevy_server::{RoomKey, UserKey};

use naia_bevy_demo_shared::messages::KeyCommand;

#[derive(Resource)]
pub struct Global {
    pub main_room_key: RoomKey,
    pub user_to_prediction_map: HashMap<UserKey, Entity>,
    pub player_last_command: HashMap<Entity, KeyCommand>,
}
