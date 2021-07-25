use std::{
    rc::Rc,
    collections::HashMap,
};

use hecs::*;

use naia_server::{Server, ServerConfig, ServerEvent, UserKey, RoomKey, EntityKey, Ref};

use naia_example_shared::{
    get_shared_config, manifest_load,
    components::{Components, Position, Name},
    events::{Events, StringMessage},
};

pub struct App {
    server: Server<Events, Components>,
    world: World,
    main_room_key: RoomKey,
    tick_count: u32,
    entity_key_map: HashMap<EntityKey, Entity>,
}

impl App {
    pub async fn new(server_config: ServerConfig) -> Self {

        let mut server = Server::new(
            manifest_load(),
            Some(server_config),
            get_shared_config(),
        )
        .await;

        // This method is called during the connection handshake process, and can be
        // used to reject a new connection if the correct credentials have not been
        // provided
        server.on_auth(Rc::new(Box::new(|_, auth_type| {
            if let Events::Auth(auth) = auth_type {
                let username = auth.username.get();
                let password = auth.password.get();
                return username == "charlie" && password == "12345";
            }
            return false;
        })));

        // Create a new, singular room, which will contain Users and Entities that they
        // can receive updates from
        let main_room_key = server.create_room();

        let mut world = World::new();
        let mut entity_key_map = HashMap::new();

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

                // Create an entity
                let entity_key = server.register_entity();
                server.room_add_entity(&main_room_key, &entity_key);

                // Add position component to Entity
                let position = Position::new((count * 4) as u8, 0).wrap();
                let _pos_key = server.add_component_to_entity(&entity_key, Components::Position(position.clone()));

                // Add name component to Entity
                let name = Name::new(first, last).wrap();
                let _name_key = server.add_component_to_entity(&entity_key, Components::Name(name.clone()));

                // Add to World
                let hecs_entity = world.spawn((
                    Ref::clone(&name),
                    Ref::clone(&position),
                ));

                entity_key_map.insert(entity_key, hecs_entity);
            }
        }

        App {
            server,
            world,
            entity_key_map,
            main_room_key,
            tick_count: 0,
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
                            }
                        }
                        ServerEvent::Disconnection(_, user) => {
                            info!("Naia Server disconnected from: {:?}", user.address);
                        }
                        ServerEvent::Event(user_key, event_type) => {
                            if let Some(user) = self.server.get_user(&user_key) {
                                match event_type {
                                    Events::StringMessage(event) => {
                                        let message = event.message.get();
                                        info!("Naia Server recv <- {}: {}", user.address, message);
                                    }
                                    _ => {}
                                }
                            }
                        }
                        ServerEvent::Tick => {

                            // Game logic, march entities across the screen
                            for (_, position_ref) in self.world.query_mut::<&Ref<Position>>() {
                                let mut position = position_ref.borrow_mut();
                                let mut x = *position.x.get();
                                x += 1;
                                if x > 20 {
                                    x = 0;
                                }
                                if x % 3 == 0 {
                                    let mut y = *position.y.get();
                                    y = y.wrapping_add(1);
                                    position.y.set(y);
                                }
                                position.x.set(x);
                            }

                            // Update scopes of entities
                            for (room_key, user_key, entity_key) in self.server.entity_scope_sets() {
                                if let Some(entity) = self.entity_key_map.get(&entity_key) {
                                    if let Ok(pos_ref) = self.world.get::<Ref<Position>>(*entity) {
                                        let x = *pos_ref.borrow().x.get();
                                        let in_scope = x >= 5 && x <= 15;
                                        self.server.entity_set_scope(&room_key, &user_key, &entity_key, in_scope);
                                    }
                                }
                            }

                            // Event Sending
                            let mut iter_vec: Vec<UserKey> = Vec::new();
                            for (user_key, _) in self.server.users_iter() {
                                iter_vec.push(user_key);
                            }
                            for user_key in iter_vec {
                                let user = self.server.get_user(&user_key).unwrap();
                                let new_message = format!("Server Packet (tick {})", self.tick_count);
                                info!("Naia Server send -> {}: {}", user.address, new_message);

                                let message_event = StringMessage::new(new_message);
                                self.server.queue_event(&user_key, &message_event);
                            }

                            // VERY IMPORTANT! Calling this actually sends all Actor/Event data
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
