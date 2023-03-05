use bevy_app::App;
use bevy_asset::AssetPlugin;
use bevy_core::{TaskPoolPlugin, TypeRegistrationPlugin, FrameCountPlugin};
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
        .add_plugin(TaskPoolPlugin::default())
        .add_plugin(TypeRegistrationPlugin::default())
        .add_plugin(FrameCountPlugin::default())
        .add_plugin(TimePlugin::default())
        .add_plugin(TransformPlugin::default())
        .add_plugin(InputPlugin::default())
        .add_plugin(WindowPlugin::default())
        .add_plugin(bevy_a11y::AccessibilityPlugin)
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

        // first
        .add_system(events::connect_events)
        .add_system(events::disconnect_events)
        .add_system(events::reject_events)
        .add_system(events::spawn_entity_events)
        .add_system(events::despawn_entity_events)
        .add_system(events::insert_component_events)
        .add_system(events::update_component_events)
        .add_system(events::remove_component_events)
        .add_system(events::message_events)
        .add_system(events::tick_events)
        // second
        .add_system(input::server_input)
        .add_system(input::client_input)
        // third
        .add_system(sync)
        // Run App
        .run();
}
