pub use naia_client::{ClientConfig, OwnedEntity, Random};

pub use naia_bevy_shared::{Entity, Stage};

mod client;
pub mod components;
pub mod events;
mod flag;
mod plugin;
mod resource;
mod stage;
mod state;
mod systems;

pub use client::Client;
pub use plugin::Plugin;
