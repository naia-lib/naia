use std::time::Duration;

use bevy_app::{App, ScheduleRunnerPlugin, Startup, TaskPoolPlugin, Update};
use bevy_diagnostic::FrameCountPlugin;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_log::{info, LogPlugin};

use naia_bevy_demo_shared::{
    components::{Color, Position, Shape},
    protocol,
};

use naia_bevy_server::{
    AppRegisterComponentEvents, HandleWorldEvents, Plugin as ServerPlugin, ServerConfig,
};

mod resources;
mod systems;

use systems::{events, init};

fn main() {
    info!("Naia Bevy Server Demo starting up");

    let mut server_config = ServerConfig::default();
    server_config.connection.disconnection_timeout_duration = Duration::from_secs(10);

    // Build App
    App::default()
        // Plugins
        .add_plugins(TaskPoolPlugin::default())
        .add_plugins(FrameCountPlugin::default())
        // this is needed to avoid running the server at uncapped FPS
        .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_millis(3)))
        .add_plugins(LogPlugin::default())
        .add_plugins(ServerPlugin::new(server_config, protocol()))
        // Startup System
        .add_systems(Startup, init)
        // Add Component Events
        .add_component_events::<Position>()
        .add_component_events::<Color>()
        .add_component_events::<Shape>()
        // Receive Server Events
        .add_systems(
            Update,
            (
                events::auth_events,
                events::connect_events,
                events::disconnect_events,
                events::error_events,
                events::tick_events,
                events::spawn_entity_events,
                events::despawn_entity_events,
                events::publish_entity_events,
                events::unpublish_entity_events,
                events::insert_component_events,
                events::update_component_events,
                events::remove_component_events,
                events::request_events,
                events::response_events,
            )
                .chain()
                .in_set(HandleWorldEvents),
        )
        // Run App
        .run();
}
