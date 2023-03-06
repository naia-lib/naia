use bevy_app::App;
use bevy_asset::AssetPlugin;
use bevy_core::{FrameCountPlugin, TaskPoolPlugin, TypeRegistrationPlugin};
use bevy_core_pipeline::CorePipelinePlugin;
use bevy_ecs::schedule::{IntoSystemConfigs, IntoSystemSetConfig, SystemSet};
use bevy_input::InputPlugin;
use bevy_log::LogPlugin;
use bevy_render::{texture::ImagePlugin, RenderPlugin};
use bevy_sprite::SpritePlugin;
use bevy_time::TimePlugin;
use bevy_transform::TransformPlugin;
use bevy_window::WindowPlugin;
use bevy_winit::WinitPlugin;

use naia_bevy_client::{ClientConfig, Plugin as ClientPlugin, ReceiveEvents};
use naia_bevy_demo_shared::protocol;

use crate::systems::{events, init, input, sync};

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
struct MainLoop;

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
        // Receive Client Events
        .add_systems(
            (
                events::connect_events,
                events::disconnect_events,
                events::reject_events,
                events::spawn_entity_events,
                events::despawn_entity_events,
                events::insert_component_events,
                events::update_component_events,
                events::remove_component_events,
                events::message_events,
                events::tick_events,
            )
                .chain()
                .in_set(ReceiveEvents),
        )
        // Realtime Gameplay Loop
        .configure_set(MainLoop.after(ReceiveEvents))
        .add_systems(
            (input::server_input, input::client_input, sync)
                .chain()
                .in_set(MainLoop),
        )
        // Run App
        .run();
}
