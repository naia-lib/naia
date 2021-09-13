use std::collections::HashMap;

use naia_server::{EntityKey, Event, Random, RoomKey, Server, ServerConfig, UserKey};

use naia_macroquad_demo_shared::{
    behavior as shared_behavior, get_server_address, get_shared_config,
    protocol::{Color, Protocol, Square},
};

pub struct App {
    server: Server<Protocol>,
    main_room_key: RoomKey,
    user_to_prediction_map: HashMap<UserKey, EntityKey>,
}

impl App {
    pub fn new() -> Self {
        info!("Naia Macroquad Server Demo started");

        let shared_config = get_shared_config();
        let mut server_config = ServerConfig::default();
        server_config.socket_config.session_listen_addr = get_server_address();

        let mut server = Server::new(Some(server_config), shared_config);

        // Create a new, singular room, which will contain Users and Entities that they
        // can receive updates from
        let main_room_key = server.make_room().key();

        App {
            server,
            main_room_key,
            user_to_prediction_map: HashMap::<UserKey, EntityKey>::new(),
        }
    }

    pub fn update(&mut self) {
        for event in self.server.receive() {
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
                        .spawn_entity()
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
                            .entity_mut(&entity_key)
                            .disown()
                            .leave_room(&self.main_room_key)
                            .despawn();
                    }
                }
                Ok(Event::Command(_, entity_key, Protocol::KeyCommand(key_command_ref))) => {
                    if let Some(square_ref) = self.server.entity(&entity_key).component::<Square>()
                    {
                        shared_behavior::process_command(&key_command_ref, &square_ref);
                    }
                }
                Ok(Event::Tick) => {
                    // All game logic should happen here, on a tick event

                    // Check whether Entities are in/out of all possible Scopes
                    for (room_key, user_key, entity_key) in self.server.scopes() {
                        // You'd normally do whatever checks you need to in here..
                        // to determine whether each Entity should be in scope or not.

                        // This indicates the Entity should be in this scope.
                        self.server.accept_scope(room_key, user_key, entity_key);

                        // And call this if Entity should NOT be in this scope.
                        // self.server.reject_scope(...);
                    }

                    // VERY IMPORTANT! Calling this actually sends all update data
                    // packets to all Clients that require it. If you don't call this
                    // method, the Server will never communicate with it's connected Clients
                    self.server.send_all_updates();
                }
                Err(error) => {
                    info!("Naia Server error: {}", error);
                }
                _ => {}
            }
        }
    }
}
