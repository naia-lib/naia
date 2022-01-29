use bevy::prelude::*;

use crate::resources::Global;
use naia_bevy_demo_shared::{get_server_address, protocol::{Protocol, Auth}};
use naia_bevy_client::Client;

pub fn init(mut commands: Commands, mut client: Client<Protocol>) {
    info!("Naia Bevy Client Demo started");

    client.auth(Auth::new("charlie", "12345"));
    client.connect(get_server_address());

    // Setup Camera
    commands.spawn_bundle(OrthographicCameraBundle::new_2d());

    // Setup Colors
    commands.insert_resource(Global {
        queued_command: None,
    });
}

