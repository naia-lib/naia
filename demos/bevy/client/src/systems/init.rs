use bevy::prelude::{
    info, shape, Assets, Camera2dBundle, Color, ColorMaterial, Commands, Mesh, ResMut,
};

use naia_bevy_client::{transport::webrtc, Client};
use naia_bevy_demo_shared::messages::Auth;

use crate::resources::Global;

pub fn init(
    mut commands: Commands,
    mut client: Client,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    info!("Naia Bevy Client Demo started");

    client.auth(Auth::new("charlie", "12345"));
    let socket = webrtc::Socket::new("http://127.0.0.1:14191", client.socket_config());
    client.connect(socket);

    // Setup Camera
    commands.spawn(Camera2dBundle::default());

    // Setup Global Resource
    let mut global = Global::default();

    // Load colors
    global.red = materials.add(ColorMaterial::from(Color::RED));
    global.blue = materials.add(ColorMaterial::from(Color::BLUE));
    global.yellow = materials.add(ColorMaterial::from(Color::YELLOW));
    global.green = materials.add(ColorMaterial::from(Color::GREEN));
    global.white = materials.add(ColorMaterial::from(Color::WHITE));
    global.purple = materials.add(ColorMaterial::from(Color::PURPLE));
    global.orange = materials.add(ColorMaterial::from(Color::ORANGE));
    global.aqua = materials.add(ColorMaterial::from(Color::AQUAMARINE));

    // Load shapes
    global.circle = meshes.add(shape::Circle::new(6.).into());

    // Insert Global Resource
    commands.insert_resource(global);
}
