use bevy::{log::LogPlugin, prelude::*};
use naia_bevy_demo_shared::get_shared_config;
use naia_bevy_server::{Plugin as ServerPlugin, ServerConfig, Stage};

mod resources;
mod systems;

use systems::{events, init, tick};

fn main() {
    info!("Naia Bevy Server Demo starting up");

    // Build App
    let mut app = App::new();

    app
    // Plugins
    .add_plugins(MinimalPlugins)
    .add_plugin(LogPlugin::default())
    .add_plugin(ServerPlugin::new(ServerConfig::default(), get_shared_config()))

    // Startup System
    .add_startup_system(
        init)
    // Receive Server Events
    .add_system_to_stage(
        Stage::ReceiveEvents,
        events::authorization_event)
    .add_system_to_stage(
        Stage::ReceiveEvents,
        events::connection_event)
    .add_system_to_stage(
        Stage::ReceiveEvents,
        events::disconnection_event)
    .add_system_to_stage(
        Stage::ReceiveEvents,
        events::command_event)
    // Gameplay Loop on Tick
    .add_system_to_stage(
        Stage::Tick,
        tick)

    // Run App
    .run();
}
