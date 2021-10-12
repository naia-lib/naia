use std::collections::HashSet;

use hecs::World;

use naia_hecs_server::{ServerAddrs, ServerConfig, SharedConfig, WorldProxy, WorldProxyMut};

use naia_hecs_demo_shared::protocol::{Name, Position, Protocol};

use crate::app::{App, Server};

pub fn app_init(
    server_config: ServerConfig,
    shared_config: SharedConfig<Protocol>,
    server_addrs: ServerAddrs,
) -> App {
    let mut server = Server::new(server_config, shared_config);
    server.listen(server_addrs);

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
                .spawn_entity(&mut world.proxy_mut())
                .enter_room(&main_room_key)
                .insert_component(&position_ref)
                .insert_component(&name_ref)
                .id();
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
