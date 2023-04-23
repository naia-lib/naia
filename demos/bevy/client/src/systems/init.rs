use bevy::prelude::{
    info, shape, Assets, Camera2dBundle, Color, ColorMaterial, Commands, Mesh, ResMut,
};

use naia_bevy_client::{transport::webrtc, Client, CommandsExt, Random};
use naia_bevy_demo_shared::{components::Baseline, messages::Auth};

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
    global.big_circle = meshes.add(shape::Circle::new(18.).into());

    // Set up new baseline entity
    let baseline = {
        let x = 16 * ((Random::gen_range_u32(0, 40) as i16) - 20);
        let y = 16 * ((Random::gen_range_u32(0, 30) as i16) - 15);
        Baseline::new(x, y)
    };

    global.baseline_entity = Some(
        commands
            // Spawn new Entity
            .spawn_empty()
            // MUST call this to begin replication
            .enable_replication(&mut client)
            // Insert Baseline component
            .insert(baseline)
            // return Entity id
            .id(),
    );

    // Insert Global Resource
    commands.insert_resource(global);
}
