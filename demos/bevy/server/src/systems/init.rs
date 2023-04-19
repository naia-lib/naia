use std::collections::HashMap;

use bevy_ecs::system::Commands;
use bevy_log::info;
use naia_bevy_demo_shared::components::Position;

use naia_bevy_server::{transport::webrtc, CommandsExt, Random, Server};

use crate::resources::Global;

pub fn init(mut commands: Commands, mut server: Server) {
    info!("Naia Bevy Server Demo is running");

    // Naia Server initialization
    let server_addresses = webrtc::ServerAddrs::new(
        "127.0.0.1:14191"
            .parse()
            .expect("could not parse Signaling address/port"),
        // IP Address to listen on for UDP WebRTC data channels
        "127.0.0.1:14192"
            .parse()
            .expect("could not parse WebRTC data address/port"),
        // The public WebRTC IP address to advertise
        "http://127.0.0.1:14192",
    );
    let socket = webrtc::Socket::new(&server_addresses, server.socket_config());
    server.listen(socket);

    // Create a new, singular room, which will contain Users and Entities that they
    // can receive updates from
    let main_room_key = server.make_room().key();

    // Set up new baseline entity
    let position = {
        let x = 16 * ((Random::gen_range_u32(0, 40) as i16) - 20);
        let y = 16 * ((Random::gen_range_u32(0, 30) as i16) - 15);
        Position::new(x, y)
    };

    let baseline_entity = commands
        // Spawn new Entity
        .spawn_empty()
        // MUST call this to begin replication
        .enable_replication(&mut server)
        // Insert Position component
        .insert(position)
        // return Entity id
        .id();

    // Init Global Resource
    let global = Global {
        main_room_key,
        user_to_square_map: HashMap::new(),
        user_to_cursor_map: HashMap::new(),
        client_to_server_cursor_map: HashMap::new(),
        baseline_entity,
    };

    // Add baseline entity to main room
    server
        .room_mut(&global.main_room_key)
        .add_entity(&baseline_entity);

    // Insert Global Resource
    commands.insert_resource(global);
}
