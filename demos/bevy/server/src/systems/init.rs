use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
};

use bevy_ecs::system::Commands;
use bevy_log::info;

use naia_bevy_server::{transport::webrtc, Server};

use crate::resources::Global;

#[expect(
    clippy::expect_used,
    reason = "demo startup requires its configured server ports"
)]
pub fn init(mut commands: Commands, mut server: Server) {
    info!("Naia Bevy Server Demo is running");

    // Naia Server initialization
    let server_addresses = webrtc::ServerAddrs::new(
        SocketAddr::from(([127, 0, 0, 1], 14191)),
        // IP Address to listen on for UDP WebRTC data channels
        SocketAddr::from(([127, 0, 0, 1], 14192)),
        // The public WebRTC IP address to advertise
        "http://127.0.0.1:14192",
    );
    let socket = webrtc::Socket::new(&server_addresses, server.socket_config());
    server.listen(socket).expect("server listener must bind");

    // Create a new, singular room, which will contain Users and Entities that they
    // can receive updates from
    let main_room_key = server.create_room().key();

    // Init Global Resource
    let global = Global {
        main_room_key,
        user_to_square_map: HashMap::new(),
        square_to_user_map: HashMap::new(),
        response_keys: HashSet::new(),
        request_index: 0,
    };

    // Insert Global Resource
    commands.insert_resource(global);
}
