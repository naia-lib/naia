use std::collections::{HashMap, HashSet};

use hecs::{Entity as HecsEntityKey, World};

use naia_server::{
    EntityKey as NaiaEntityKey, EntityKey, Event, Ref, Replicate, RoomKey, Server, ServerAddrs,
    ServerConfig,
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
    has_marker: HashSet<NaiaEntityKey>,
}

impl App {
    pub fn new() -> Self {
        info!("Naia Hecs Server Demo started");

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

                // Create Position component
                let position_ref = Position::new((count * 4) as u8, 0);

                // Create Name component
                let name_ref = Name::new(first, last);

                // Create an Entity
                let naia_entity_key = server
                    .spawn_entity()
                    .enter_room(&main_room_key)
                    .insert_component(&position_ref)
                    .insert_component(&name_ref)
                    .key();

                // Add to World
                let hecs_entity_key =
                    world.spawn((Ref::clone(&name_ref), Ref::clone(&position_ref)));

                naia_to_hecs_key_map.insert(naia_entity_key, hecs_entity_key);
                hecs_to_naia_key_map.insert(hecs_entity_key, naia_entity_key);
            }
        }

        App {
            server,
            world,
            naia_to_hecs_key_map,
            hecs_to_naia_key_map,
            main_room_key,
            tick_count: 0,
            has_marker: HashSet::new(),
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
                    let address = self
                        .server
                        .user_mut(&user_key)
                        .enter_room(&self.main_room_key)
                        .address();
                    info!("Naia Server connected to: {}", address);
                }
                Ok(Event::Disconnection(_, user)) => {
                    info!("Naia Server disconnected from: {:?}", user.address);
                }
                Ok(Event::Message(user_key, Protocol::StringMessage(message_ref))) => {
                    let address = self.server.user(&user_key).address();
                    let message = message_ref.borrow();
                    let message_inner = message.message.get();
                    info!("Naia Server recv <- {}: {}", address, message_inner);
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
                    while let Some(hecs_entity_key) = entities_to_add.pop() {
                        let naia_entity_key = self
                            .hecs_to_naia_key_map
                            .get(&hecs_entity_key)
                            .expect("hecs <-> naia map not working ..");

                        if !self.has_marker.contains(naia_entity_key) {
                            // Create Marker component
                            let marker = Marker::new("new");

                            // Add to Hecs World
                            self.world
                                .insert_one(hecs_entity_key, Ref::clone(&marker))
                                .expect("error inserting!");

                            // Add to Naia Server
                            self.server
                                .entity_mut(&naia_entity_key)
                                .insert_component(&marker);

                            // Track that this entity has a Marker
                            self.has_marker.insert(*naia_entity_key);
                        }
                    }

                    // remove marker
                    while let Some(hecs_entity_key) = entities_to_remove.pop() {
                        let naia_entity_key: EntityKey = *self
                            .hecs_to_naia_key_map
                            .get(&hecs_entity_key)
                            .expect("hecs <-> naia map not working ..");

                        if self.has_marker.remove(&naia_entity_key) {
                            // Remove from Hecs World
                            self.hecs_remove_component::<Marker>(&hecs_entity_key);

                            // Remove from Naia Server
                            self.server
                                .entity_mut(&naia_entity_key)
                                .remove_component::<Marker>();
                        }
                    }

                    // Update scopes of entities
                    for (_, user_key, naia_entity_key) in self.server.scope_checks() {
                        if let Some(hecs_entity_key) =
                            self.naia_to_hecs_key_map.get(&naia_entity_key)
                        {
                            if let Ok(pos_ref) = self.world.get::<Ref<Position>>(*hecs_entity_key) {
                                let x = *pos_ref.borrow().x.get();
                                if x >= 5 && x <= 100 {
                                    self.server.user_scope(&user_key).include(&naia_entity_key);
                                } else {
                                    self.server.user_scope(&user_key).exclude(&naia_entity_key);
                                }
                            }
                        }
                    }

                    // Message Sending
                    for user_key in self.server.user_keys() {
                        let address = self.server.user(&user_key).address();
                        let message_contents = format!("Server Packet (tick {})", self.tick_count);
                        info!("Naia Server send -> {}: {}", address, message_contents);

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

    fn hecs_remove_component<R: 'static + Replicate<Protocol>>(
        &mut self,
        hecs_entity_key: &HecsEntityKey,
    ) {
        self.world
            .remove_one::<Ref<R>>(*hecs_entity_key)
            .expect("error removing component");
    }
}
