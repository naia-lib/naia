use std::default::Default;

use bevy_ecs::prelude::Resource;

use naia_bevy_shared::Flag;

#[derive(Default, Resource)]
pub struct ClientResource {
    pub ticker: Flag,
    pub connector: Flag,
    pub disconnector: Flag,
    pub rejector: Flag,
}
