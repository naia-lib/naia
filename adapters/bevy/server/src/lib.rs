//! Bevy adapter for the naia server.
//!
//! Adds naia's replication and messaging into a Bevy application. Entities and
//! components that carry the [`Replicate`] marker are automatically tracked and
//! replicated to connected clients; no manual diff loop is required.
//!
//! # Setup
//!
//! Add the plugin and call [`listen_on_app`] (or call [`Server::listen`] in a
//! startup system):
//!
//! ```no_run
//! # use bevy_app::App;
//! # use naia_bevy_server::Plugin;
//! fn main() {
//!     App::new()
//!         // .add_plugins(DefaultPlugins)
//!         .add_plugins(Plugin::new(server_config(), protocol()))
//!         // .add_systems(Startup, init)
//!         .run();
//! }
//! # fn server_config() -> naia_bevy_server::ServerConfig { todo!() }
//! # fn protocol() -> naia_bevy_shared::Protocol { todo!() }
//! ```
//!
//! Interact with the server via the [`Server`] Bevy resource, or use
//! [`CommandsExt`] / [`ServerCommandsExt`] on [`Commands`] to spawn entities
//! and configure replication.
//!
//! # Quick start
//!
//! ```no_run
//! use bevy_app::{App, Startup, Update};
//! use bevy_ecs::prelude::*;
//! use naia_bevy_server::{
//!     events::ConnectEvent,
//!     transport::webrtc,
//!     Plugin, Server, ServerConfig, UserKey,
//! };
//! use naia_bevy_shared::Protocol;
//!
//! fn main() {
//!     App::new()
//!         .add_plugins(Plugin::new(ServerConfig::default(), Protocol::builder().build()))
//!         .add_systems(Startup, init)
//!         .add_systems(Update, on_connect)
//!         .run();
//! }
//!
//! fn init(mut server: Server) {
//!     server.listen(webrtc::Socket::new(&server_addrs(), None));
//! }
//!
//! fn on_connect(mut server: Server, mut connect_events: EventReader<ConnectEvent>) {
//!     for ConnectEvent(user_key) in connect_events.read() {
//!         server.accept_connection(user_key);
//!         // server.user_mut(user_key).enter_room(&room_key);
//!     }
//! }
//! # fn server_addrs() -> naia_bevy_server::transport::webrtc::ServerAddrs { todo!() }
//! ```
//!
//! # Key types
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`Plugin`] | Registers systems and the [`Server`] resource |
//! | [`Server`] | Bevy-wrapped server resource |
//! | [`CommandsExt`] | Extension methods on [`Commands`] for replication setup |
//! | [`ServerCommandsExt`] | Server-only extension methods on [`Commands`] |
//! | [`events`] | Bevy events mirroring naia world events |
//!
//! [`Commands`]: bevy_ecs::system::Commands
//! [`Replicate`]: naia_bevy_shared::Replicate

pub use naia_bevy_shared::{
    EntityAuthStatus, HandleTickEvents, HandleWorldEvents, Random, Replicate, ReplicateBundle,
    Tick, WorldUpdate,
};
pub use naia_server::{
    shared::{
        default_channels, BigMap, BigMapKey, BitReader, BitWrite, BitWriter, ConstBitLength,
        FileBitWriter, ResponseReceiveKey, SerdeErr, SignedInteger, SignedVariableInteger,
        SocketConfig, UnsignedInteger, UnsignedVariableInteger,
    },
    transport, ReplicationConfig, RoomKey, SerdeBevy as Serde, ServerConfig, UserKey,
};

pub mod events;

mod app_ext;
mod bundle_event_registry;
mod commands;
mod component_event_registry;
mod components;
mod plugin;
#[doc(hidden)]
mod resource_sync;
mod server;
mod systems;

pub use app_ext::AppRegisterComponentEvents;
pub use commands::{CommandsExt, ServerCommandsExt};
pub use components::{ClientOwned, ServerOwned};
pub use plugin::Plugin;
pub use server::Server;

/// Call `listen` on the naia server resource directly via the App,
/// before any systems run. Use this when you want to initialize the
/// server during app construction rather than in a startup system.
pub fn listen_on_app<S: Into<Box<dyn transport::Socket>>>(app: &mut bevy_app::App, socket: S) {
    app.world_mut()
        .resource_mut::<server::ServerImpl>()
        .listen(socket);
}

/// Phantom tag type for single-server Bevy apps.
///
/// Pass this as the `T` parameter to [`Plugin`], [`Server`], and event types
/// when your app connects to exactly one server instance. For multi-server
/// apps define your own tag structs instead.
pub struct DefaultServerTag;

/// Alias for [`Plugin`] — for single-server apps.
pub type DefaultPlugin = Plugin;
