pub use naia_bevy_shared::{Random, Tick};
pub use naia_server::{RoomKey, ServerAddrs, ServerConfig, UserKey};

use bevy_ecs::component::Component;

pub mod events;

mod plugin;
mod server;
mod systems;

pub use plugin::Plugin;
pub use server::Server;

#[derive(Component)]
pub struct ServerOwned;