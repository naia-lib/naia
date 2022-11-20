use std::default::Default;

use bevy_ecs::prelude::Resource;

use naia_bevy_shared::Flag;

#[derive(Default, Resource)]
pub struct ServerResource {
    pub ticker: Flag,
}
