use std::time::Duration;
use bevy_app::{App, ScheduleRunnerPlugin, ScheduleRunnerSettings};
use bevy_core::CorePlugin;
use bevy_log::{info, LogPlugin};

use naia_bevy_demo_shared::protocol;
use naia_bevy_server::{Plugin as ServerPlugin, ServerConfig};

mod resources;
mod systems;

use systems::{events, init};

fn main() {
    info!("Naia Bevy Server Demo starting up");

    // Build App
    App::default()
        // Plugins
        .add_plugin(CorePlugin::default())
        .insert_resource(
            // this is needed to avoid running the server at uncapped FPS
            ScheduleRunnerSettings::run_loop(Duration::from_secs_f64(1.0 / 60.0))
        )
        .add_plugin(ScheduleRunnerPlugin::default())
        .add_plugin(LogPlugin::default())
        .add_plugin(ServerPlugin::new(ServerConfig::default(), protocol()))
        // Startup System
        .add_startup_system(init)
        // Receive Server Events
        .add_system(events::auth_events)
        .add_system(events::connect_events)
        .add_system(events::disconnect_events)
        .add_system(events::error_events)
        .add_system(events::tick_events)
        .add_system(events::spawn_entity_events)
        .add_system(events::insert_component_events)
        .add_system(events::update_component_events)
        // Run App
        .run();
}
