pub use naia_client::{ClientConfig, Event, Random, Ref};

pub use naia_bevy_shared::{Entity, Stage};

mod client;
pub mod components;
mod plugin;
mod resource;
mod stage;
mod state;
mod systems;

pub use client::Client;
pub use plugin::Plugin;
