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
