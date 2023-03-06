pub use naia_bevy_shared::{Random, Tick};
pub use naia_server::{RoomKey, ServerAddrs, ServerConfig, UserKey};

pub mod events;

mod commands;
mod plugin;
mod server;
mod systems;

pub use commands::CommandsExt;
pub use plugin::Plugin;
pub use server::Server;

pub type ServerOwned = naia_bevy_shared::HostOwned;
