use bevy_app::{App, CoreStage};
use bevy_asset::AssetPlugin;
use bevy_core::CorePlugin;
use bevy_core_pipeline::CorePipelinePlugin;
use bevy_input::InputPlugin;
use bevy_log::LogPlugin;
use bevy_render::{texture::ImagePlugin, RenderPlugin};
use bevy_sprite::SpritePlugin;
use bevy_time::TimePlugin;
use bevy_transform::TransformPlugin;
use bevy_window::WindowPlugin;
use bevy_winit::WinitPlugin;

use naia_bevy_client::{ClientConfig, Plugin as ClientPlugin};
use naia_bevy_demo_shared::protocol;

use crate::systems::{events, init, input, sync};

pub fn run() {
    App::default()
        // Bevy Plugins
        .add_plugin(LogPlugin::default())
        .add_plugin(CorePlugin::default())
        .add_plugin(TimePlugin::default())
        .add_plugin(TransformPlugin::default())
        .add_plugin(InputPlugin::default())
        .add_plugin(WindowPlugin::default())
        .add_plugin(AssetPlugin::default())
        .add_plugin(WinitPlugin::default())
        .add_plugin(RenderPlugin::default())
        .add_plugin(ImagePlugin::default())
        .add_plugin(CorePipelinePlugin::default())
        .add_plugin(SpritePlugin::default())
        // Add Naia Client Plugin
        .add_plugin(ClientPlugin::new(ClientConfig::default(), protocol()))
        // Startup System
        .add_startup_system(init)
        // Realtime Gameplay Loop
        .add_system_to_stage(CoreStage::PreUpdate, events::connect_events)
        .add_system_to_stage(CoreStage::PreUpdate, events::disconnect_events)
        .add_system_to_stage(CoreStage::PreUpdate, events::reject_events)
        .add_system_to_stage(CoreStage::PreUpdate, events::spawn_entity_events)
        .add_system_to_stage(CoreStage::PreUpdate, events::despawn_entity_events)
        .add_system_to_stage(CoreStage::PreUpdate, events::insert_component_events)
        .add_system_to_stage(CoreStage::PreUpdate, events::update_component_events)
        .add_system_to_stage(CoreStage::PreUpdate, events::remove_component_events)
        .add_system_to_stage(CoreStage::PreUpdate, events::message_events)
        .add_system_to_stage(CoreStage::PreUpdate, events::tick_events)
        .add_system_to_stage(CoreStage::Update, input::server_input)
        .add_system_to_stage(CoreStage::Update, input::client_input)
        .add_system_to_stage(CoreStage::Update, sync)
        // Run App
        .run();
}
