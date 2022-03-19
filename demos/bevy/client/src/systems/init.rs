use bevy::prelude::*;
use naia_bevy_client::Client;
use naia_bevy_demo_shared::protocol::{Auth, Protocol};

use crate::resources::Global;

pub fn init(mut commands: Commands, mut client: Client<Protocol>) {
    info!("Naia Bevy Client Demo started");

    client.auth(Auth::new("charlie", "12345"));
    client.connect("http://127.0.0.1:14191");

    // Setup Camera
    commands.spawn_bundle(OrthographicCameraBundle::new_2d());

    // Setup Colors
    commands.insert_resource(Global {
        queued_command: None,
    });
}
