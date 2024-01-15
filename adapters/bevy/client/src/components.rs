use bevy_ecs::component::Component;

use naia_bevy_shared::HostOwned;

pub type ClientOwned<T: Send + Sync + 'static> = HostOwned<T>;

#[derive(Component)]
pub struct ServerOwned;
