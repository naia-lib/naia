use bevy_ecs::component::Component;

use naia_bevy_shared::HostOwned;
use naia_server::UserKey;

pub type ServerOwned = HostOwned;

#[derive(Component)]
pub struct ClientOwned(pub UserKey);
