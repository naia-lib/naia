//! Bevy adapter for the naia client.
//!
//! Integrates naia's entity replication and messaging into a Bevy application.
//! Server-replicated entities appear in the Bevy world automatically; client-
//! authoritative entities can be spawned and published via [`CommandsExt`].
//!
//! Runs natively (UDP) and in the browser (`wasm32-unknown-unknown` + WebRTC).
//!
//! # Single-client apps (most common)
//!
//! Use [`DefaultPlugin`] and [`DefaultClientTag`] — no phantom type needed:
//!
//! ```no_run
//! # use bevy_app::App;
//! # use naia_bevy_client::{DefaultPlugin, DefaultClientTag, Client};
//! # use naia_bevy_shared::Protocol;
//! fn main() {
//!     App::new()
//!         // .add_plugins(DefaultPlugins)
//!         .add_plugins(DefaultPlugin::new(client_config(), protocol()))
//!         // .add_systems(Startup, init)  // call Client::connect here
//!         .run();
//! }
//!
//! fn my_system(mut client: Client<DefaultClientTag>) {
//!     // use client here
//! }
//! # fn client_config() -> naia_bevy_client::ClientConfig { todo!() }
//! # fn protocol() -> Protocol { todo!() }
//! ```
//!
//! # Multi-client apps
//!
//! Disambiguate each server connection with a distinct phantom tag type:
//!
//! ```no_run
//! # use bevy_app::App;
//! # use naia_bevy_client::{Plugin, Client};
//! # use naia_bevy_shared::Protocol;
//! struct LobbyTag;
//! struct GameTag;
//!
//! fn main() {
//!     App::new()
//!         .add_plugins(Plugin::<LobbyTag>::new(lobby_config(), protocol()))
//!         .add_plugins(Plugin::<GameTag>::new(game_config(), protocol()))
//!         .run();
//! }
//!
//! fn lobby_system(mut lobby: Client<LobbyTag>) { /* ... */ }
//! fn game_system(mut game: Client<GameTag>) { /* ... */ }
//! # fn lobby_config() -> naia_bevy_client::ClientConfig { todo!() }
//! # fn game_config() -> naia_bevy_client::ClientConfig { todo!() }
//! # fn protocol() -> Protocol { todo!() }
//! ```
//!
//! # Key types
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`DefaultPlugin`] | Single-client plugin (alias for `Plugin<DefaultClientTag>`) |
//! | [`Plugin`] | Generic plugin for multi-client apps |
//! | [`Client`] | Bevy-wrapped client `SystemParam` |
//! | [`DefaultClientTag`] | Phantom type for single-client apps |
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

/// Phantom tag type for single-client Bevy apps.
///
/// Pass this as the `T` parameter to [`Plugin`], [`Client`], and event types
/// when your app connects to exactly one server. For multi-client apps define
/// your own tag structs instead.
pub struct DefaultClientTag;

/// Alias for [`Plugin<DefaultClientTag>`] — for single-client apps.
pub type DefaultPlugin = Plugin<DefaultClientTag>;
