use std::collections::HashMap;

use bevy_ecs::system::Commands;
use bevy_log::info;
use naia_bevy_demo_shared::components::{Baseline, Color, ColorValue, Shape, ShapeValue};

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

    // Set up new baseline entities
    let baseline_1_component = {
        let x = 16 * ((Random::gen_range_u32(0, 40) as i16) - 20);
        let y = 16 * ((Random::gen_range_u32(0, 30) as i16) - 15);
        Baseline::new(x, y)
    };

    let baseline_1_entity = commands
        // Spawn new Entity
        .spawn_empty()
        // MUST call this to begin replication
        .enable_replication(&mut server)
        // Insert Baseline component
        .insert(baseline_1_component)
        // Insert Color component
        .insert(Color::new(ColorValue::Purple))
        // Insert Shape component (Big Circle)
        .insert(Shape::new(ShapeValue::BigCircle))
        // return Entity id
        .id();

    let baseline_2_component = {
        let x = 16 * ((Random::gen_range_u32(0, 40) as i16) - 20);
        let y = 16 * ((Random::gen_range_u32(0, 30) as i16) - 15);
        Baseline::new(x, y)
    };

    let baseline_2_entity = commands
        // Spawn new Entity
        .spawn_empty()
        // MUST call this to begin replication
        .enable_replication(&mut server)
        // Insert Baseline component
        .insert(baseline_2_component)
        // Insert Color component
        .insert(Color::new(ColorValue::Orange))
        // Insert Shape component (Big Circle)
        .insert(Shape::new(ShapeValue::BigCircle))
        // return Entity id
        .id();

    // Init Global Resource
    let global = Global {
        main_room_key,
        user_to_square_map: HashMap::new(),
        user_to_cursor_map: HashMap::new(),
        client_to_server_cursor_map: HashMap::new(),
        server_baseline_1: baseline_1_entity,
        server_baseline_2: baseline_2_entity,
        client_baselines: HashMap::new(),
        square_to_user_map: HashMap::new(),
    };

    // Add baseline entity 1 to main room
    server
        .room_mut(&global.main_room_key)
        .add_entity(&baseline_1_entity);

    // Add baseline entity 2 to main room
    server
        .room_mut(&global.main_room_key)
        .add_entity(&baseline_2_entity);

    // Insert Global Resource
    commands.insert_resource(global);
}
