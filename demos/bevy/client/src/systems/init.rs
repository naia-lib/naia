use bevy::{
    color::LinearRgba,
    log::info,
    prelude::{Assets, Camera2d, Circle, Color, ColorMaterial, Commands, Mesh, ResMut},
};
use naia_bevy_client::{transport::webrtc, Client};

use naia_bevy_demo_shared::messages::Auth;

use crate::{app::Main, resources::Global};

pub fn init(
    mut commands: Commands,
    mut client: Client<Main>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    info!("Naia Bevy Client Demo started");

    client.auth(Auth::new("charlie", "12345"));
    let socket = webrtc::Socket::new("http://127.0.0.1:14191", client.socket_config());
    client.connect(socket);

    // Setup Camera
    commands.spawn(Camera2d);

    // Setup Global Resource
    let global = Global {
        red: materials.add(ColorMaterial::from(Color::LinearRgba(LinearRgba::RED))),
        blue: materials.add(ColorMaterial::from(Color::LinearRgba(LinearRgba::BLUE))),
        yellow: materials.add(ColorMaterial::from(Color::LinearRgba(LinearRgba::rgb(
            1.0, 1.0, 0.0,
        )))),
        green: materials.add(ColorMaterial::from(Color::LinearRgba(LinearRgba::GREEN))),
        white: materials.add(ColorMaterial::from(Color::LinearRgba(LinearRgba::WHITE))),
        purple: materials.add(ColorMaterial::from(Color::LinearRgba(LinearRgba::rgb(
            1.0, 0.0, 1.0,
        )))),
        orange: materials.add(ColorMaterial::from(Color::LinearRgba(LinearRgba::rgb(
            1.0, 0.5, 0.0,
        )))),
        aqua: materials.add(ColorMaterial::from(Color::LinearRgba(LinearRgba::rgb(
            0.0, 1.0, 1.0,
        )))),
        circle: meshes.add(Circle::new(6.)),
        ..Default::default()
    };

    // Insert Global Resource
    commands.insert_resource(global);
}
