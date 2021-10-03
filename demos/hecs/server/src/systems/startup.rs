use std::collections::HashSet;

use hecs::World;

use naia_server::{ServerAddrs, ServerConfig};

use naia_hecs_demo_shared::{
    get_server_address, get_shared_config,
    protocol::{Name, Position},
};

use naia_hecs_server::WorldProxy;

use crate::app::{App, Server};

pub fn app_init() -> App {
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

    let mut server = Server::new(ServerConfig::default(), get_shared_config());
    server.listen(server_addresses);

    // Create a new, singular room, which will contain Users and Entities that they
    // can receive updates from
    let main_room_key = server.make_room().key();

    let mut world = World::new();

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
            let position_ref = Position::new((count * 4) as u8, 0);

            // Create Name component
            let name_ref = Name::new(first, last);

            // Create an Entity
            server
                .spawn_entity(world.proxy())
                .enter_room(&main_room_key)
                .insert_component(&position_ref)
                .insert_component(&name_ref)
                .key();
        }
    }

    App {
        server,
        world,
        main_room_key,
        tick_count: 0,
        has_marker: HashSet::new(),
    }
}
