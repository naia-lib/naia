use std::{collections::HashMap, time::Duration};

use naia_server::{
    Event, ObjectKey, Random, RoomKey, Server, ServerAddresses, ServerConfig, UserKey,
};

use naia_macroquad_demo_shared::{
    behavior as shared_behavior, get_server_address, get_shared_config,
    protocol::{Color, Protocol, Square},
};

pub struct App {
    server: Server<Protocol>,
    main_room_key: RoomKey,
    user_to_pawn_map: HashMap<UserKey, ObjectKey>,
}

impl App {
    pub fn new() -> Self {
        info!("Naia Macroquad Server Demo started");

        let mut server_config = ServerConfig::default();
        server_config.socket_addresses = ServerAddresses::new(
            // IP Address to listen on for the signaling portion of WebRTC
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
        server_config.heartbeat_interval = Duration::from_secs(2);
        // Keep in mind that the disconnect timeout duration should always be at least
        // 2x greater than the client's heartbeat interval, to make it so that at the
        // worst case, the server would need to miss 2 client heartbeats before
        // disconnecting them
        server_config.disconnection_timeout_duration = Duration::from_secs(5);

        let mut server =
            Server::new(Protocol::load(), Some(server_config), get_shared_config());

        let main_room_key = server.create_room();

        App {
            server,
            main_room_key,
            user_to_pawn_map: HashMap::<UserKey, ObjectKey>::new(),
        }
    }

    pub fn update(&mut self) {
        for event_result in self.server.receive() {
            match event_result {
                Ok(event) => {
                    match event {
                        Event::Authorization(user_key, Protocol::Auth(auth_ref)) => {
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
                        Event::Connection(user_key) => {
                            self.server.room_add_user(&self.main_room_key, &user_key);
                            if let Some(user) = self.server.get_user(&user_key) {
                                info!("Naia Server connected to: {}", user.address);

                                let x = Random::gen_range_u32(0, 50) * 16;
                                let y = Random::gen_range_u32(0, 37) * 16;

                                let square_color = match self.server.get_users_count() % 3 {
                                    0 => Color::Yellow,
                                    1 => Color::Red,
                                    _ => Color::Blue,
                                };

                                let square = Square::new(x as u16, y as u16, square_color);
                                let square_key = self.server.register_object(&square);
                                self.server
                                    .room_add_object(&self.main_room_key, &square_key);
                                self.server.assign_pawn(&user_key, &square_key);
                                self.user_to_pawn_map.insert(user_key, square_key);
                            }
                        }
                        Event::Disconnection(user_key, user) => {
                            info!("Naia Server disconnected from: {:?}", user.address);
                            self.server.room_remove_user(&self.main_room_key, &user_key);
                            if let Some(object_key) = self.user_to_pawn_map.remove(&user_key) {
                                self.server
                                    .room_remove_object(&self.main_room_key, &object_key);
                                self.server.unassign_pawn(&user_key, &object_key);
                                self.server.deregister_object(&object_key);
                            }
                        }
                        Event::Command(_, square_key, Protocol::KeyCommand(key_command_ref)) => {
                            if let Some(Protocol::Square(square_ref)) =
                                self.server.get_object(&square_key)
                            {
                                shared_behavior::process_command(&key_command_ref, square_ref);
                            }
                        }
                        Event::Tick => {
                            // All game logic should happen here, on a tick event

                            // Update scopes of objects
                            for (room_key, user_key, object_key) in self.server.object_scope_sets() {
                                self.server
                                    .object_set_scope(&room_key, &user_key, &object_key, true);
                            }

                            // VERY IMPORTANT! Calling this actually sends all update data
                            // packets to all Clients that require it. If you don't call this
                            // method, the Server will never communicate with it's connected Clients
                            self.server.send_all_updates();
                        }
                        _ => {}
                    }
                }
                Err(error) => {
                    info!("Naia Server error: {}", error);
                }
            }
        }
    }
}
