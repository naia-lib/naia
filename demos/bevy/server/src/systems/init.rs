use std::collections::HashMap;

use bevy::prelude::*;

use naia_bevy_server::Server;

use naia_bevy_demo_shared::protocol::Protocol;

use crate::resources::Global;

pub fn init(mut commands: Commands, mut server: Server<Protocol>) {
    info!("Naia Bevy Server Demo is running");

    // Create a new, singular room, which will contain Users and Entities that they
    // can receive updates from
    let main_room_key = server.make_room().key();

    // Resources
    commands.insert_resource(Global {
        main_room_key,
        user_to_prediction_map: HashMap::new(),
    })
}
