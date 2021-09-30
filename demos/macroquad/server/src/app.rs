use std::collections::HashMap;

use naia_server::{
    Event, Random, RoomKey, Server as NaiaServer, ServerAddrs, ServerConfig, UserKey, WorldType,
};

use naia_server_default_world::World as DefaultWorld;

use naia_macroquad_demo_shared::{
    behavior as shared_behavior, get_server_address, get_shared_config,
    protocol::{Color, Protocol, Square},
};

type World = DefaultWorld<Protocol>;
type Server = NaiaServer<Protocol, World>;
type EntityKey = <World as WorldType<Protocol>>::EntityKey;

pub struct App {
    server: Server,
    world: World,
    main_room_key: RoomKey,
    user_to_prediction_map: HashMap<UserKey, EntityKey>,
}

impl App {
    pub fn new() -> Self {
        info!("Naia Macroquad Server Demo started");

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
            user_to_prediction_map: HashMap::<UserKey, EntityKey>::new(),
        }
    }

    pub fn update(&mut self) {
        for event in self.server.receive(&self.world) {
            match event {
                Ok(Event::Authorization(user_key, Protocol::Auth(auth_ref))) => {
                    let auth_message = auth_ref.borrow();
                    let username = auth_message.username.get();
                    let password = auth_message.password.get();
                    if username == "charlie" && password == "12345" {
                        // Accept incoming connection
                        self.server.accept_connection(&user_key);
                    } else {
                        // Reject incoming connection
                        self.server.reject_connection(&user_key);
                    }
                }
                Ok(Event::Connection(user_key)) => {
                    let user_address = self
                        .server
                        .user_mut(&user_key)
                        .enter_room(&self.main_room_key)
                        .address();

                    info!("Naia Server connected to: {}", user_address);

                    let x = Random::gen_range_u32(0, 50) * 16;
                    let y = Random::gen_range_u32(0, 37) * 16;

                    let square_color = match self.server.users_count() % 3 {
                        0 => Color::Yellow,
                        1 => Color::Red,
                        _ => Color::Blue,
                    };

                    let square = Square::new(x as u16, y as u16, square_color);
                    let entity_key = self
                        .server
                        .spawn_entity(&mut self.world)
                        .insert_component(&square)
                        .set_owner(&user_key)
                        .enter_room(&self.main_room_key)
                        .key();
                    self.user_to_prediction_map.insert(user_key, entity_key);
                }
                Ok(Event::Disconnection(user_key, user)) => {
                    info!("Naia Server disconnected from: {:?}", user.address);
                    self.server
                        .user_mut(&user_key)
                        .leave_room(&self.main_room_key);
                    if let Some(entity_key) = self.user_to_prediction_map.remove(&user_key) {
                        self.server
                            .entity_mut(&mut self.world, &entity_key)
                            .disown()
                            .leave_room(&self.main_room_key)
                            .despawn();
                    }
                }
                Ok(Event::Command(_, entity_key, Protocol::KeyCommand(key_command_ref))) => {
                    if let Some(square_ref) = self
                        .server
                        .entity(&self.world, &entity_key)
                        .component::<Square>()
                    {
                        shared_behavior::process_command(&key_command_ref, &square_ref);
                    }
                }
                Ok(Event::Tick) => {
                    // All game logic should happen here, on a tick event

                    // Check whether Entities are in/out of all possible Scopes
                    for (_, user_key, entity_key) in self.server.scope_checks() {
                        // You'd normally do whatever checks you need to in here..
                        // to determine whether each Entity should be in scope or not.

                        // This indicates the Entity should be in this scope.
                        self.server.user_scope(&user_key).include(&entity_key);

                        // And call this if Entity should NOT be in this scope.
                        // self.server.user_scope(..).exclude(..);
                    }

                    // VERY IMPORTANT! Calling this actually sends all update data
                    // packets to all Clients that require it. If you don't call this
                    // method, the Server will never communicate with it's connected Clients
                    self.server.send_all_updates(&self.world);
                }
                Err(error) => {
                    info!("Naia Server error: {}", error);
                }
                _ => {}
            }
        }
    }
}
