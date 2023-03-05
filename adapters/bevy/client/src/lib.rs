pub use naia_bevy_shared::{sequence_greater_than, Random, Tick};
pub use naia_client::{ClientConfig, CommandHistory};

pub mod events;

mod client;
mod plugin;
mod systems;
mod commands;

pub use client::Client;
pub use plugin::Plugin;
pub use commands::CommandsExt;

use bevy_ecs::component::Component;

#[derive(Component)]
pub struct ClientOwned;