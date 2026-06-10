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
//! use bevy_ecs::message::MessageReader;
//! use naia_bevy_server::{
//!     events::ConnectEvent,
//!     transport, Plugin, Server, ServerConfig, UserKey,
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
//!     // pick a concrete transport: transport::webrtc, transport::udp, transport::local
//!     let socket: Box<dyn transport::Socket> = todo!();
//!     server.listen(socket);
//! }
//!
//! fn on_connect(mut server: Server, mut events: MessageReader<ConnectEvent>) {
//!     for ConnectEvent(user_key) in events.read() {
//!         server.accept_connection(&user_key);
//!         // server.user_mut(&user_key).enter_room(&room_key);
//!     }
//! }
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

// Re-exported so consumers (e.g. cyberlith `game_cell`) depend only on this
// adapter crate and never reach past it into `naia-bevy-shared` / `naia-shared`
// directly. `SnapshotWorld` / `IdentityToken` originate in `naia-shared` but are
// surfaced here via `naia-bevy-shared` (the bevy shared layer re-exports them).
pub use naia_bevy_shared::{
    ComponentKind, EntityAndGlobalEntityConverter, EntityAuthStatus, HandleTickEvents,
    HandleWorldEvents, HostOwned, HostSyncEvent, HostSyncOwnedAddedTracking, IdentityToken, Instant,
    ProcessPackets, Random, ReceivePackets, ReplicaDynRefWrapper, ReplicaRefWrapper, Replicate,
    ReplicateBundle, ReplicatedComponent, ResponseSendKey, SendPackets, SnapshotWorld, Tick,
    TranslateWorldEvents, WorldMutType, WorldOpCommand, WorldProxy, WorldProxyMut, WorldRef,
    WorldRefType, WorldToHostSync, WorldUpdate,
};
pub use naia_server::{
    pipeline_actors,
    shared::{
        default_channels, BigMap, BigMapKey, BitReader, BitWrite, BitWriter, ConstBitLength,
        FileBitWriter, ResponseReceiveKey, SerdeErr, SignedInteger, SignedVariableInteger,
        SocketConfig, UnsignedInteger, UnsignedVariableInteger,
    },
    transport, ConnectionShared, EntityOwner, ReceiveOutput, RecvHandle, ScopeExit, SendHandle,
    ReplicationConfig, RoomKey, SerdeBevy as Serde, ServerConfig, TickBufferMessages, UserKey,
    WorldServer,
};

#[cfg(feature = "bench_instrumentation")]
pub use naia_server::{bench_iris_counters, bench_send_counters, bench_scope_counters, bench_serde_counters, bench_take_events_counters, bench_write_counters};

pub mod events;
pub mod pipeline_timing;

mod app_ext;
mod apply_receive_output;
mod bundle_event_registry;
mod commands;
mod component_event_registry;
mod components;
mod host_sync_pipeline;
mod plugin;
mod plugin_full;
#[doc(hidden)]
mod resource_sync;
mod server;
mod sim_converter;
mod systems;

pub use app_ext::AppRegisterComponentEvents;
pub use apply_receive_output::{
    apply_receive_output_pipeline, apply_receive_output_pipeline_with_sim_receiver,
    apply_receive_output_pipeline_with_sim_receiver_split,
};
pub use commands::{CommandsExt, ServerCommandsExt};
pub use component_event_registry::ComponentEventRegistry;
pub use components::{ClientOwned, ServerOwned};
pub use host_sync_pipeline::drain_host_sync_into_pipeline;
pub use plugin::Plugin;
pub use plugin_full::{
    drain_recv_impl, drain_recv_impl_split, SimHandleRes, PluginInternalState, PluginSimConfig, RecvHandleRes,
    SendHandleRes, SimEventReceiverRes, SnapshotReceiverRes, SnapshotSenderRes,
};
pub use server::Server;
pub use sim_converter::SimConverter;

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
