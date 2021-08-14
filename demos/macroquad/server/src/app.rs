use std::{
    collections::HashMap,
    rc::Rc,
};

use naia_server::{ObjectKey, Server, Random, ServerConfig, ServerEvent, UserKey, RoomKey};

use naia_demo_macroquad_shared::{
    get_shared_config, behavior as shared_behavior, protocol::{Protocol, Point, Color},
};

pub struct App {
    server: Server<Protocol>,
    main_room_key: RoomKey,
    user_to_pawn_map: HashMap::<UserKey, ObjectKey>,
}

impl App {
    pub async fn new(server_config: ServerConfig) -> Self {

        let mut server = Server::new(
            Protocol::load(),
            Some(server_config),
            get_shared_config(),
        )
        .await;

        server.on_auth(Rc::new(Box::new(|_, auth_type| {
            if let Protocol::Auth(auth_ref) = auth_type {
                let auth_event = auth_ref.borrow();
                let username = auth_event.username.get();
                let password = auth_event.password.get();
                return username == "charlie" && password == "12345";
            }
            return false;
        })));

        let main_room_key = server.create_room();

        App {
            server,
            main_room_key,
            user_to_pawn_map: HashMap::<UserKey, ObjectKey>::new(),
        }
    }

    pub async fn update(&mut self) {
        match self.server.receive().await {
            Ok(event) => {
                match event {
                    ServerEvent::Connection(user_key) => {
                        self.server.room_add_user(&self.main_room_key, &user_key);
                        if let Some(user) = self.server.get_user(&user_key) {
                            info!("Naia Server connected to: {}", user.address);

                            let x = Random::gen_range_u32(0, 50) * 16;
                            let y = Random::gen_range_u32(0, 37) * 16;

                            let replicate_color = match self.server.get_users_count() % 3 {
                                0 => Color::Yellow,
                                1 => Color::Red,
                                _ => Color::Blue,
                            };

                            let new_replicate =
                                Point::new(x as u16, y as u16, replicate_color).wrap();
                            let new_object_key = self.server
                                .register_replicate(Protocol::Point(new_replicate.clone()));
                            self.server.room_add_replicate(&self.main_room_key, &new_object_key);
                            self.server.assign_pawn(&user_key, &new_object_key);
                            self.user_to_pawn_map.insert(user_key, new_object_key);
                        }
                    }
                    ServerEvent::Disconnection(user_key, user) => {
                        info!("Naia Server disconnected from: {:?}", user.address);
                        self.server.room_remove_user(&self.main_room_key, &user_key);
                        if let Some(object_key) = self.user_to_pawn_map.remove(&user_key) {
                            self.server.room_remove_replicate(&self.main_room_key, &object_key);
                            self.server.unassign_pawn(&user_key, &object_key);
                            self.server.deregister_replicate(object_key);
                        }
                    }
                    ServerEvent::Command(_, object_key, command_type) => match command_type {
                        Protocol::KeyCommand(key_command) => {
                            if let Some(typed_replicate) = self.server.get_object(object_key) {
                                match typed_replicate {
                                    Protocol::Point(replicate) => {
                                        shared_behavior::process_command(&key_command, replicate);
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    },
                    ServerEvent::Tick => {
                        // Update scopes of entities
                            for (room_key, user_key, object_key) in self.server.replicate_scope_sets() {
                                self.server.replicate_set_scope(&room_key, &user_key, &object_key, true);
                            }

                        // VERY IMPORTANT! Calling this actually sends all Replicate/Event data
                        // packets to all Clients that require it. If you don't call this
                        // method, the Server will never communicate with it's connected Clients
                        self.server.send_all_updates().await;

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
