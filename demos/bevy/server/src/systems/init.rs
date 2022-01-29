use std::collections::HashMap;

use bevy::prelude::*;

use naia_bevy_server::Server;

use naia_bevy_demo_shared::{get_server_address, protocol::Protocol};
use naia_bevy_server::ServerAddrs;

use crate::resources::Global;

pub fn init(mut commands: Commands, mut server: Server<Protocol>) {
    info!("Naia Bevy Server Demo is running");

    // Naia Server initialization
    let server_addresses = ServerAddrs::new(
        get_server_address(),
        // IP Address to listen on for UDP WebRTC data channels
        "127.0.0.1:14192"
            .parse()
            .expect("could not parse WebRTC data address/port"),
        // The public WebRTC IP address to advertise
        "127.0.0.1:14192"
            .parse()
            .expect("could not parse advertised public WebRTC data address/port"),
    );

    server.listen(server_addresses);

    // Create a new, singular room, which will contain Users and Entities that they
    // can receive updates from
    let main_room_key = server.make_room().key();

    // Resources
    commands.insert_resource(Global {
        main_room_key,
        user_to_prediction_map: HashMap::new(),
    })
}
