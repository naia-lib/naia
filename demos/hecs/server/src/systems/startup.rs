use std::collections::HashSet;

use hecs::World;

use naia_hecs_server::{Protocol, transport::webrtc, ServerConfig};

use naia_hecs_demo_shared::{Name, Position};

use crate::app::{App, Server};

pub fn app_init(
    server_config: ServerConfig,
    mut protocol: Protocol,
    server_addrs: webrtc::ServerAddrs,
) -> App {
    let mut world = protocol.wrap_world(World::new());

    let socket = webrtc::Socket::new(&server_addrs, protocol.socket_config());
    let mut server = Server::new(server_config, protocol);
    server.listen(socket);

    // Create a new, singular room, which will contain Users and Entities that they
    // can receive updates from
    let main_room_key = server.make_room().key();

    {
        let mut count = 0;
        for (first, last) in [
            ("alpha", "red"),
            ("bravo", "blue"),
            ("charlie", "green"),
            ("delta", "yellow"),
        ]
        .iter()
        {
            count += 1;

            // Create Position component
            let position_ref = Position::new((count * 5) as u8, 0);

            // Create Name component
            let name_ref = Name::new(first, last);

            // Create an Entity
            server
                .spawn_entity(&mut world)
                .enter_room(&main_room_key)
                .insert_component(position_ref)
                .insert_component(name_ref)
                .id();
        }
    }

    App {
        has_user: false,
        server,
        world,
        main_room_key,
        tick_count: 0,
        has_marker: HashSet::new(),
    }
}
