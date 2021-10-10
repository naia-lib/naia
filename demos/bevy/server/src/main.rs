use std::collections::HashMap;

use bevy::{log::LogPlugin, prelude::*};

use naia_bevy_server::{Plugin, ServerAddrs, ServerConfig};

use naia_bevy_demo_shared::{get_server_address, get_shared_config};

mod aliases;
mod resources;
mod systems;

use aliases::Server;
use resources::Global;
use systems::{check_scopes, receive_events, send_updates, should_tick, tick};

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

    let mut app = App::build();

    // Plugins
    app.add_plugins(MinimalPlugins)
        .add_plugin(LogPlugin::default())
        .add_plugin(Plugin::new(ServerConfig::default(), get_shared_config(), server_addresses))

    // Systems
    .add_startup_system(init.system())
    .add_system_to_stage(CoreStage::PreUpdate,
                         receive_events.system())
    .add_system_to_stage(CoreStage::PostUpdate,
                         tick.system().chain(
                             check_scopes.system().chain(
                                 send_updates.system()))
                             .with_run_criteria(should_tick.system()))

    // Run
    .run();
}

fn init(mut commands: Commands, mut server: Server) {
    info!("Naia Bevy Server Demo is running");

    // Create a new, singular room, which will contain Users and Entities that they
    // can receive updates from
    let main_room_key = server.make_room().key();

    // Resources
    commands.insert_resource(Global {
        main_room_key,
        user_to_prediction_map: HashMap::new(),
    })
}
