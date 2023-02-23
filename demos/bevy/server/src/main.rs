use bevy_app::{App, ScheduleRunnerPlugin};
use bevy_core::CorePlugin;
use bevy_log::{info, LogPlugin};

use naia_bevy_demo_shared::protocol;
use naia_bevy_server::{Plugin as ServerPlugin, ServerConfig};

mod resources;
mod systems;

use systems::{events, init, tick_events};

fn main() {
    info!("Naia Bevy Server Demo starting up");

    // Build App
    App::default()
        // Plugins
        .add_plugin(CorePlugin::default())
        .add_plugin(ScheduleRunnerPlugin::default())
        .add_plugin(LogPlugin::default())
        .add_plugin(ServerPlugin::new(ServerConfig::default(), protocol()))
        // Startup System
        .add_startup_system(init)
        // Receive Server Events
        .add_system(events::auth_events)
        .add_system(events::connect_events)
        .add_system(events::disconnect_events)
        .add_system(events::message_events)
        .add_system(events::error_events)
        .add_system(tick_events)
        // Run App
        .run();
}
