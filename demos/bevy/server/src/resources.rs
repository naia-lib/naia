use std::collections::HashMap;

use bevy_ecs::{entity::Entity, prelude::Resource};

use naia_bevy_server::{RoomKey, UserKey};

#[derive(Resource)]
pub struct Global {
    pub main_room_key: RoomKey,
    pub user_to_square_map: HashMap<UserKey, Entity>,
    pub user_to_cursor_map: HashMap<UserKey, Entity>,
    pub client_to_server_cursor_map: HashMap<Entity, Entity>,
    pub server_baseline_1: Entity,
    pub server_baseline_2: Entity,
    pub client_baselines: HashMap<UserKey, Entity>,
    pub square_to_user_map: HashMap<Entity, UserKey>,
}
