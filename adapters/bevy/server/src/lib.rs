pub use naia_bevy_shared::{Random, Tick};
pub use naia_server::{RoomKey, ServerAddrs, ServerConfig, UserKey};

pub mod events;

mod plugin;
mod server;
mod systems;
mod commands;

pub use plugin::Plugin;
pub use server::Server;
pub use commands::CommandsExt;

pub type ServerOwned = naia_bevy_shared::HostOwned;