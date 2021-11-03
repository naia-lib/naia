use std::collections::HashMap;

use naia_server::{
    Event, Random, RoomKey, Server as NaiaServer, ServerAddrs, ServerConfig, UserKey,
};

use naia_default_world::{Entity, World as DefaultWorld};

use naia_macroquad_demo_shared::{
    behavior as shared_behavior, get_server_address, get_shared_config,
    protocol::{Color, Protocol, Square},
};

type World = DefaultWorld<Protocol>;
type Server = NaiaServer<Protocol, Entity>;

pub struct App {
    server: Server,
    world: World,
    main_room_key: RoomKey,
}

impl App {
    pub fn new() -> Self {
        info!("Naia Tickless Server Demo started");

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

        let world = World::new();

        // Create a new, singular room, which will contain Users and Entities that they
        // can receive updates from
        let main_room_key = server.make_room().key();

        App {
            server,
            world,
            main_room_key,
        }
    }

    pub fn update(&mut self) {
        for event in self.server.receive() {
            match event {
                Ok(Event::Connection(user_key)) => {
                    let user_address = self
                        .server
                        .user_mut(&user_key)
                        .enter_room(&self.main_room_key)
                        .address();

                    info!("Naia Server connected to: {}", user_address);
                }
                Ok(Event::Disconnection(user_key, user)) => {
                    info!("Naia Server disconnected from: {:?}", user.address);
                }
                Ok(Event::Message(_, entity, Protocol::Text(text))) => {
                    info!("message: {}", text.value.get());
                }
                Ok(Event::Tick) => {
                    info!("TICK SHOULD NOT HAPPEN!");
//                    // All game logic should happen here, on a tick event
//
//                    // Check whether Entities are in/out of all possible Scopes
//                    for (_, user_key, entity) in self.server.scope_checks() {
//                        // You'd normally do whatever checks you need to in here..
//                        // to determine whether each Entity should be in scope or not.
//
//                        // This indicates the Entity should be in this scope.
//                        self.server.user_scope(&user_key).include(&entity);
//
//                        // And call this if Entity should NOT be in this scope.
//                        // self.server.user_scope(..).exclude(..);
//                    }
//
//                    // VERY IMPORTANT! Calling this actually sends all update data
//                    // packets to all Clients that require it. If you don't call this
//                    // method, the Server will never communicate with it's connected Clients
//                    self.server.send_all_updates(self.world.proxy());
                }
                Err(error) => {
                    info!("Naia Server error: {}", error);
                }
                _ => {}
            }
        }
    }
}
