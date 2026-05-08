//! Bevy adapter for the naia client.
//!
//! Integrates naia's entity replication and messaging into a Bevy application.
//! Server-replicated entities appear in the Bevy world automatically; client-
//! authoritative entities can be spawned and published via [`CommandsExt`].
//!
//! Runs natively (UDP) and in the browser (`wasm32-unknown-unknown` + WebRTC).
//!
//! # Setup
//!
//! ```no_run
//! # use bevy_app::App;
//! # use naia_bevy_client::Plugin;
//! fn main() {
//!     App::new()
//!         // .add_plugins(DefaultPlugins)
//!         .add_plugins(Plugin::new(client_config(), protocol()))
//!         // .add_systems(Startup, init)  // call Client::connect here
//!         .run();
//! }
//! # fn client_config() -> naia_bevy_client::ClientConfig { todo!() }
//! # fn protocol() -> naia_shared::Protocol { todo!() }
//! ```
//!
//! Access the server connection via the [`Client`] Bevy resource, or use
//! [`CommandsExt`] / [`ClientCommandsExt`] on [`Commands`] to configure entity
//! replication and authority.
//!
//! # Key types
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`Plugin`] | Registers systems and the [`Client`] resource |
//! | [`Client`] | Bevy-wrapped client resource |
//! | [`CommandsExt`] | Extension methods on [`Commands`] for replication |
//! | [`ClientCommandsExt`] | Client-only extension methods on [`Commands`] |
//! | [`events`] | Bevy events mirroring naia world events |
//!
//! [`Commands`]: bevy_ecs::system::Commands

pub use naia_bevy_shared::{
    sequence_greater_than, sequence_less_than, wrapping_diff, EntityAuthStatus, GameInstant,
    HandleTickEvents, HandleWorldEvents, Random, Replicate, ReplicateBundle, ResponseSendKey, Tick,
    Timer, WorldUpdate,
};
pub use naia_client::{
    shared::{default_channels, Instant, Message, ResponseReceiveKey},
    transport, ClientConfig, CommandHistory, JitterBufferType, NaiaClientError, Publicity,
};

pub mod events;

mod app_ext;
mod bundle_event_registry;
mod client;
mod commands;
mod component_event_registry;
mod components;
mod plugin;
mod resource_sync;
mod systems;

pub use app_ext::AppRegisterComponentEvents;
pub use client::{Client, ClientWrapper};
pub use commands::{CommandsExt, ClientCommandsExt};
pub use components::{ClientOwned, ServerOwned};
pub use plugin::Plugin;
