use std::time::Duration;

use naia_server::{Event, RoomKey, Server, ServerAddresses, ServerConfig, UserKey};

use naia_basic_demo_shared::{
    get_server_address, get_shared_config,
    protocol::{Character, Protocol, StringMessage},
};

pub struct App {
    server: Server<Protocol>,
    main_room_key: RoomKey,
    tick_count: u32,
}

impl App {
    pub async fn new() -> Self {
        info!("Basic Naia Server Demo started");

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
            Server::new(Protocol::load(), Some(server_config), get_shared_config()).await;

        // Create a new, singular room, which will contain Users and Objects that they
        // can receive updates from
        let main_room_key = server.create_room();

        // Create 4 Character objects, with a range of X and name values
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

                // Create a Character
                let character =
                    Character::new((count * 4) as u8, 0, first, last);
                let character_key = server.register_object(character);

                // Add the Character to the main Room
                server.room_add_object(&main_room_key, &character_key);
            }
        }

        App {
            server,
            main_room_key,
            tick_count: 0,
        }
    }

    pub async fn update(&mut self) {
        match self.server.receive().await {
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
                        if let Some(user) = self.server.get_user(&user_key) {
                            info!("Naia Server connected to: {}", user.address);
                            self.server.room_add_user(&self.main_room_key, &user_key);
                        }
                    }
                    Event::Disconnection(_, user) => {
                        info!("Naia Server disconnected from: {:?}", user.address);
                    }
                    Event::Message(user_key, Protocol::StringMessage(message_ref)) => {
                        if let Some(user) = self.server.get_user(&user_key) {
                            let message = message_ref.borrow();
                            let message_contents = message.contents.get();
                            info!(
                                "Server recv from ({}) <- {}",
                                user.address, message_contents
                            );
                        }
                    }
                    Event::Tick => {
                        // All game logic should happen here, on a tick event

                        // Message sending
                        let mut iter_vec: Vec<UserKey> = Vec::new();
                        for (user_key, _) in self.server.users_iter() {
                            iter_vec.push(user_key);
                        }
                        for user_key in iter_vec {
                            let user = self.server.get_user(&user_key).unwrap();
                            let new_message_contents =
                                format!("Server Message ({})", self.tick_count);
                            info!(
                                "Server send to   ({}) -> {}",
                                user.address, new_message_contents
                            );

                            let new_message = StringMessage::new(new_message_contents);
                            self.server.queue_message(&user_key, new_message, true);
                        }

                        // Iterate through Characters, marching them from (0,0) to (20, N)
                        for object_key in self.server.objects_iter() {
                            if let Some(Protocol::Character(character_ref)) =
                                self.server.get_object(object_key)
                            {
                                character_ref.borrow_mut().step();
                            }
                        }

                        // Update scopes of objects
                        for (room_key, user_key, object_key) in self.server.object_scope_sets() {
                            if let Some(Protocol::Character(character_ref)) =
                                self.server.get_object(&object_key)
                            {
                                let x = *character_ref.borrow().x.get();
                                let in_scope = x >= 5 && x <= 15;
                                self.server.object_set_scope(
                                    &room_key,
                                    &user_key,
                                    &object_key,
                                    in_scope,
                                );
                            }
                        }

                        // VERY IMPORTANT! Calling this actually sends all update data
                        // packets to all Clients that require it. If you don't call this
                        // method, the Server will never communicate with it's connected Clients
                        self.server.send_all_updates().await;

                        self.tick_count = self.tick_count.wrapping_add(1);
                    }
                    _ => {}
                }
            }
            Err(error) => {
                info!("Naia Server Error: {}", error);
            }
        }
    }
}
