use bevy::{log::LogPlugin, prelude::*};

use naia_bevy_server::{Plugin as ServerPlugin, ServerAddrs, ServerConfig};

use naia_bevy_demo_shared::{get_server_address, get_shared_config};

mod resources;
mod systems;

use systems::{check_scopes, init, receive_events, send_updates, should_tick, tick};

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
    let mut app = App::build();

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
        CoreStage::PreUpdate,
        receive_events.system())
    // Gameplay Loop on Tick
    .add_system_to_stage(
        CoreStage::PostUpdate,
        tick.system()
            .chain(
                check_scopes.system())
            .chain(
                send_updates.system())
            .with_run_criteria(
                should_tick.system()))

    // Run App
    .run();
}
