use bevy_ecs::component::Component;

use naia_bevy_shared::HostOwned;

pub type ClientOwned = HostOwned;

#[derive(Component)]
pub struct ServerOwned;
