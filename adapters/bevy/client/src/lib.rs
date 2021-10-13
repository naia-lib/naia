pub use naia_client::{ClientConfig, Event, Random, Ref, OwnedEntity};

pub use naia_bevy_shared::{Entity, Stage};

mod client;
pub mod components;
mod plugin;
mod resource;
mod stage;
mod state;
mod systems;
mod events;
mod flag;

pub use client::Client;
pub use plugin::Plugin;
