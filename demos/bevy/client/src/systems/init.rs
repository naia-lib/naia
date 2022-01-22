use bevy::prelude::*;

use crate::resources::Global;

pub fn init(mut commands: Commands) {
    info!("Naia Bevy Client Demo started");

    // Setup Camera
    commands.spawn_bundle(OrthographicCameraBundle::new_2d());

    // Setup Colors
    commands.insert_resource(Global {
        queued_command: None,
    });
}
