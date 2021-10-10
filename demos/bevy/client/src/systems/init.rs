use bevy::prelude::*;

use crate::resources::{Global, Materials};

pub fn init(mut commands: Commands, mut materials: ResMut<Assets<ColorMaterial>>) {
    info!("Naia Bevy Client Demo started");

    // Setup Camera
    commands.spawn_bundle(OrthographicCameraBundle::new_2d());

    // Setup Colors
    commands.insert_resource(Global {
        materials: Materials {
            white: materials.add(Color::rgb(1.0, 1.0, 1.0).into()),
            red: materials.add(Color::rgb(1.0, 0.0, 0.0).into()),
            blue: materials.add(Color::rgb(0.0, 0.0, 1.0).into()),
            yellow: materials.add(Color::rgb(1.0, 1.0, 0.0).into()),
        },
        queued_command: None,
    });
}
