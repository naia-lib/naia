pub use naia_bevy_shared::{sequence_greater_than, Random, ReceiveEvents, Tick};
pub use naia_client::{ClientConfig, CommandHistory};

pub mod events;

mod client;
mod commands;
mod plugin;
mod systems;

pub use client::Client;
pub use commands::CommandsExt;
pub use plugin::Plugin;

pub type ClientOwned = naia_bevy_shared::HostOwned;
