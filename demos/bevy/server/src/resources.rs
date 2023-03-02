use std::collections::HashMap;

use bevy_ecs::{entity::Entity, prelude::Resource};

use naia_bevy_server::{RoomKey, UserKey};

#[derive(Resource)]
pub struct Global {
    pub main_room_key: RoomKey,
    pub user_to_entity_map: HashMap<UserKey, Entity>,
    pub echo_entity_map: HashMap<Entity, Entity>,
}
