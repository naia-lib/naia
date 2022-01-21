use bevy::prelude::*;

use naia_bevy_client::{ClientConfig, Plugin as ClientPlugin, Stage};

use naia_bevy_demo_shared::{get_server_address, get_shared_config, protocol::Auth};

mod resources;
mod systems;

use systems::{events, init, input, sync, tick};

fn main() {
    let mut app = App::new();

    // Plugins
    app.add_plugins(DefaultPlugins)
        .add_plugin(ClientPlugin::new(
            ClientConfig::default(),
            get_shared_config(),
            get_server_address(),
            Some(Auth::new("charlie", "12345")),
        ));

    #[cfg(target_arch = "wasm32")]
    app.add_plugin(bevy_webgl2::WebGL2Plugin);

    app
    // Startup System
    .add_startup_system(
        init.system())
    // Realtime Gameplay Loop
    .add_system_to_stage(
        Stage::Connection,
        events::connect_event.system())
    .add_system_to_stage(
        Stage::Disconnection,
        events::disconnect_event.system())
    .add_system_to_stage(
        Stage::ReceiveEvents,
        events::spawn_entity_event.system())
    .add_system_to_stage(
        Stage::ReceiveEvents,
        events::own_entity_event.system())
    .add_system_to_stage(
        Stage::ReceiveEvents,
        events::new_command_event.system())
    .add_system_to_stage(
        Stage::ReceiveEvents,
        events::replay_command_event.system())
    .add_system_to_stage(
        Stage::Frame,
        input.system())
    .add_system_to_stage(
        Stage::PostFrame,
        sync.system())
    // Gameplay Loop on Tick
    .add_system_to_stage(
        Stage::Tick,
        tick.system())

    // Run App
    .run();
}
