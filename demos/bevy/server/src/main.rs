use bevy_app::{App, CoreStage, ScheduleRunnerPlugin};
use bevy_core::CorePlugin;
use bevy_log::{info, LogPlugin};

use naia_bevy_demo_shared::protocol;
use naia_bevy_server::{Plugin as ServerPlugin, ServerConfig, Stage};

mod resources;
mod systems;

use systems::{events, init, tick};

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
        .add_system_to_stage(CoreStage::PreUpdate, events::auth_events)
        .add_system_to_stage(CoreStage::PreUpdate, events::connect_events)
        .add_system_to_stage(CoreStage::PreUpdate, events::disconnect_events)
        .add_system_to_stage(CoreStage::PreUpdate, events::message_events)
        .add_system_to_stage(CoreStage::PreUpdate, events::error_events)
        // Gameplay Loop on Tick
        .add_system_to_stage(Stage::Tick, tick)
        // Run App
        .run();
}
