pub use naia_client::{ClientConfig, Event, Random, Ref};

pub use naia_bevy_shared::{Entity, Stage};

mod client;
mod plugin;
mod state;
mod resource;
mod systems;
pub mod components;
mod stage;

pub use client::Client;
pub use plugin::Plugin;
