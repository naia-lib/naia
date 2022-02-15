pub use naia_client::{shared::Random, ClientConfig};

pub mod components;
pub mod events;

mod client;
mod plugin;
mod resource;
mod stage;
mod state;
mod systems;

pub use client::Client;
pub use plugin::Plugin;
pub use stage::Stage;
