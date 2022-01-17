use bevy::{log::LogPlugin, prelude::*};

use naia_bevy_server::{Plugin as ServerPlugin, ServerAddrs, ServerConfig, Stage};

use naia_bevy_demo_shared::{get_server_address, get_shared_config};

mod resources;
mod systems;

use systems::{events, init, tick};

fn main() {
    info!("Naia Bevy Server Demo starting up");

    // Naia Server initialization
    let server_addresses = ServerAddrs::new(
        get_server_address(),
        // IP Address to listen on for UDP WebRTC data channels
        "127.0.0.1:14192"
            .parse()
            .expect("could not parse WebRTC data address/port"),
        // The public WebRTC IP address to advertise
        "127.0.0.1:14192"
            .parse()
            .expect("could not parse advertised public WebRTC data address/port"),
    );

    // Build App
    let mut app = App::new();

    app
    // Plugins
    .add_plugins(MinimalPlugins)
    .add_plugin(LogPlugin::default())
    .add_plugin(ServerPlugin::new(ServerConfig::default(), get_shared_config(), server_addresses))

    // Startup System
    .add_startup_system(
        init.system())
    // Receive Server Events
    .add_system_to_stage(
    Stage::ReceiveEvents,
    events::authorization_event.system())
    .add_system_to_stage(
    Stage::ReceiveEvents,
    events::connection_event.system())
    .add_system_to_stage(
    Stage::ReceiveEvents,
    events::disconnection_event.system())
    .add_system_to_stage(
        Stage::ReceiveEvents,
        events::command_event.system())
    // Gameplay Loop on Tick
    .add_system_to_stage(
        Stage::Tick,
        tick.system())

    // Run App
    .run();
}
