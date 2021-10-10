pub use naia_client::{ClientConfig, Event, LocalEntity, Random, Ref};

pub use naia_bevy_shared::Entity;

mod client;
mod plugin;
mod state;
mod ticker;

pub use client::Client;
pub use plugin::Plugin;
