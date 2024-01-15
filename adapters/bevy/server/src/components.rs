use bevy_ecs::component::Component;

use naia_bevy_shared::HostOwned;
use naia_server::UserKey;

use crate::plugin::Singleton;

pub type ServerOwned = HostOwned;

#[derive(Component)]
pub struct ClientOwned(pub UserKey);
