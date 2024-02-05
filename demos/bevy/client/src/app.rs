use bevy::{
    prelude::{
        App, ClearColor, Color, IntoSystemConfigs, IntoSystemSetConfigs, Startup, SystemSet, Update,
    },
    DefaultPlugins,
};

use naia_bevy_client::{ClientConfig, Plugin as ClientPlugin, ReceiveEvents};
use naia_bevy_demo_shared::protocol;

use crate::systems::{events, init, input, sync};

// name for the Client
pub struct Main;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
struct MainLoop;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
struct Tick;

pub fn run() {
    App::default()
        // Bevy Plugins
        .add_plugins(DefaultPlugins)
        // Add Naia Client Plugin
        .add_plugins(ClientPlugin::<Main>::new(ClientConfig::default(), protocol()))
        // Background Color
        .insert_resource(ClearColor(Color::BLACK))
        // Startup System
        .add_systems(Startup, init)
        // Receive Client Events
        .add_systems(
            Update,
            (
                events::connect_events,
                events::disconnect_events,
                events::reject_events,
                events::spawn_entity_events,
                events::despawn_entity_events,
                events::publish_entity_events,
                events::unpublish_entity_events,
                events::insert_component_events,
                events::update_component_events,
                events::remove_component_events,
                events::message_events,
                events::request_events,
            )
                .chain()
                .in_set(ReceiveEvents),
        )
        // Tick Event
        .configure_sets(Update, Tick.after(ReceiveEvents))
        .add_systems(Update, events::tick_events.in_set(Tick))
        // Realtime Gameplay Loop
        .configure_sets(Update, MainLoop.after(Tick))
        .add_systems(
            Update,
            (
                input::key_input,
                input::cursor_input,
                sync::sync_clientside_sprites,
                sync::sync_serverside_sprites,
                sync::sync_cursor_sprite,
            )
                .chain()
                .in_set(MainLoop),
        )
        // Run App
        .run();
}
