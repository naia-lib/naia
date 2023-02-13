use bevy::{
    app::{App, CoreStage},
    DefaultPlugins,
};

use naia_bevy_client::{ClientConfig, Plugin as ClientPlugin, Stage};
use naia_bevy_demo_shared::protocol;

use crate::systems::{events, init, input, sync, tick};

pub fn run() {
    App::default()
        // Plugins
        .add_plugins(DefaultPlugins)
        .add_plugin(ClientPlugin::new(ClientConfig::default(), protocol()))
        // Startup System
        .add_startup_system(init)
        // Realtime Gameplay Loop
        .add_system_to_stage(Stage::ReceiveEvents, events::connect_events)
        .add_system_to_stage(Stage::ReceiveEvents, events::disconnect_events)
        .add_system_to_stage(Stage::ReceiveEvents, events::reject_events)
        .add_system_to_stage(Stage::ReceiveEvents, events::spawn_entity_events)
        .add_system_to_stage(Stage::ReceiveEvents, events::insert_component_events)
        .add_system_to_stage(Stage::ReceiveEvents, events::update_component_events)
        .add_system_to_stage(Stage::ReceiveEvents, events::message_events)
        .add_system_to_stage(CoreStage::Update, input)
        .add_system_to_stage(CoreStage::Update, sync)
        // Gameplay Loop on Tick
        .add_system_to_stage(Stage::Tick, tick)
        // Run App
        .run();
}
