use std::collections::HashMap;

use hecs::{Entity as HecsEntityKey, World};

use naia_server::{
    ComponentKey, EntityKey as NaiaEntityKey, Event, Ref, Replicate, RoomKey, Server, ServerConfig,
    UserKey,
};

use naia_hecs_demo_shared::{
    get_server_address, get_shared_config,
    protocol::{Marker, Name, Position, Protocol, StringMessage},
};

pub struct App {
    server: Server<Protocol>,
    world: World,
    main_room_key: RoomKey,
    tick_count: u32,
    naia_to_hecs_key_map: HashMap<NaiaEntityKey, HecsEntityKey>,
    hecs_to_naia_key_map: HashMap<HecsEntityKey, NaiaEntityKey>,
    has_marker: HashMap<NaiaEntityKey, ComponentKey>,
}

impl App {
    pub fn new() -> Self {
        info!("Naia Hecs Server Demo started");

        let shared_config = get_shared_config();
        let mut server_config = ServerConfig::default();
        server_config.socket_config.session_listen_addr = get_server_address();
        let mut server = Server::new(Protocol::load(), Some(server_config), shared_config);

        // Create a new, singular room, which will contain Users and Entities that they
        // can receive updates from
        let main_room_key = server.make_room();

        let mut world = World::new();
        let mut naia_to_hecs_key_map = HashMap::new();
        let mut hecs_to_naia_key_map = HashMap::new();

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

                // Create an Entity
                let entity_key = server.create_entity();
                server.room_add_entity(&main_room_key, &entity_key);

                // Create Position component
                let position_ref = Position::new((count * 4) as u8, 0);

                // Create Name component
                let name_ref = Name::new(first, last);

                // Add to World
                let hecs_key = world.spawn((Ref::clone(&name_ref), Ref::clone(&position_ref)));

                naia_to_hecs_key_map.insert(entity_key, hecs_key);
                hecs_to_naia_key_map.insert(hecs_key, entity_key);

                // Add Position component to Entity
                let _position_component_key =
                    server.insert_component_into_entity(&entity_key, &position_ref);

                // Add Name component to Entity
                let _name_component_key =
                    server.insert_component_into_entity(&entity_key, &name_ref);
            }
        }

        App {
            server,
            world,
            naia_to_hecs_key_map,
            hecs_to_naia_key_map,
            main_room_key,
            tick_count: 0,
            has_marker: HashMap::new(),
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
                    self.server.room_add_user(&self.main_room_key, &user_key);
                    if let Some(user) = self.server.user(&user_key) {
                        info!("Naia Server connected to: {}", user.address);
                    }
                }
                Ok(Event::Disconnection(_, user)) => {
                    info!("Naia Server disconnected from: {:?}", user.address);
                }
                Ok(Event::Message(user_key, Protocol::StringMessage(message_ref))) => {
                    if let Some(user) = self.server.user(&user_key) {
                        let message = message_ref.borrow();
                        let message = message.message.get();
                        info!("Naia Server recv <- {}: {}", user.address, message);
                    }
                }
                Ok(Event::Tick) => {
                    // Game logic, march entities across the screen
                    let mut entities_to_add: Vec<HecsEntityKey> = Vec::new();
                    let mut entities_to_remove: Vec<HecsEntityKey> = Vec::new();

                    for (hecs_entity_key, position_ref) in self.world.query_mut::<&Ref<Position>>()
                    {
                        let mut position = position_ref.borrow_mut();
                        let mut x = *position.x.get();
                        x += 1;
                        if x > 125 {
                            x = 0;
                            let mut y = *position.y.get();
                            y = y.wrapping_add(1);
                            position.y.set(y);
                        }
                        if x == 40 {
                            entities_to_add.push(hecs_entity_key);
                        }
                        if x == 75 {
                            entities_to_remove.push(hecs_entity_key);
                        }
                        position.x.set(x);
                    }

                    // add marker
                    while let Some(hecs_key) = entities_to_add.pop() {
                        let naia_key = self
                            .hecs_to_naia_key_map
                            .get(&hecs_key)
                            .expect("hecs <-> naia map not working ..");

                        if !self.has_marker.contains_key(naia_key) {
                            // Create Marker component
                            let marker = Marker::new("new");

                            // Add to Hecs World
                            self.world
                                .insert_one(hecs_key, Ref::clone(&marker))
                                .expect("error inserting!");

                            // Add Marker component to Entity in Naia Server
                            let component_key =
                                self.server.insert_component_into_entity(&naia_key, &marker);

                            // Track that this entity has a Marker
                            self.has_marker.insert(*naia_key, component_key);
                        }
                    }

                    // remove marker
                    while let Some(hecs_key) = entities_to_remove.pop() {
                        let naia_key = self
                            .hecs_to_naia_key_map
                            .get(&hecs_key)
                            .expect("hecs <-> naia map not working ..");

                        if let Some(component_key) = self.has_marker.remove(naia_key) {
                            let protocol_component = self.server.remove_component(&component_key);

                            match protocol_component {
                                Protocol::Position(position_ref) => {
                                    self.remove_component(&hecs_key, &position_ref);
                                }
                                Protocol::Name(name_ref) => {
                                    self.remove_component(&hecs_key, &name_ref);
                                }
                                Protocol::Marker(marker_ref) => {
                                    self.remove_component(&hecs_key, &marker_ref);
                                }
                                _ => {}
                            }
                        }
                    }

                    // Update scopes of entities
                    for (room_key, user_key, entity_key) in self.server.scopes() {
                        if let Some(entity) = self.naia_to_hecs_key_map.get(&entity_key) {
                            if let Ok(pos_ref) = self.world.get::<Ref<Position>>(*entity) {
                                let x = *pos_ref.borrow().x.get();
                                let in_scope = x >= 5 && x <= 100;
                                self.server.entity_set_scope(
                                    &room_key,
                                    &user_key,
                                    &entity_key,
                                    in_scope,
                                );
                            }
                        }
                    }

                    // Message Sending
                    let mut iter_vec: Vec<UserKey> = Vec::new();
                    for (user_key, _) in self.server.users_iter() {
                        iter_vec.push(user_key);
                    }
                    for user_key in iter_vec {
                        let user = self.server.user(&user_key).unwrap();
                        let message_contents = format!("Server Packet (tick {})", self.tick_count);
                        info!("Naia Server send -> {}: {}", user.address, message_contents);

                        let message = StringMessage::new(message_contents);
                        self.server.queue_message(&user_key, &message, true);
                    }

                    // VERY IMPORTANT! Calling this actually sends all update data
                    // packets to all Clients that require it. If you don't call this
                    // method, the Server will never communicate with it's connected Clients
                    self.server.send_all_updates();

                    self.tick_count = self.tick_count.wrapping_add(1);
                }
                Err(error) => {
                    info!("Naia Server Error: {}", error);
                }
                _ => {}
            }
        }
    }

    fn remove_component<R: 'static + Replicate<Protocol>>(
        &mut self,
        hecs_entity_key: &HecsEntityKey,
        _component_ref: &Ref<R>,
    ) {
        self.world
            .remove_one::<Ref<R>>(*hecs_entity_key)
            .expect("error removing component");
    }
}
