use std::{thread::sleep, time::Duration};

use naia_server::{
    shared::default_channels::UnorderedReliableChannel, transport::webrtc, AuthEvent, ConnectEvent,
    DisconnectEvent, ErrorEvent, MessageEvent, RoomKey, Server as NaiaServer, ServerConfig,
    TickEvent,
};

use naia_demo_world::{Entity, World, WorldRefType};

use naia_basic_demo_shared::{protocol, Auth, Character, StringMessage};

type Server = NaiaServer<Entity>;

pub struct App {
    server: Server,
    world: World,
    main_room_key: RoomKey,
    tick_count: u32,
}

impl App {
    pub fn new() -> Self {
        info!("Basic Naia Server Demo started");

        let mut world = World::default();

        let server_addresses = webrtc::ServerAddrs::new(
            "127.0.0.1:14191"
                .parse()
                .expect("could not parse Signaling address/port"),
            // IP Address to listen on for UDP WebRTC data channels
            "127.0.0.1:14192"
                .parse()
                .expect("could not parse WebRTC data address/port"),
            // The public WebRTC IP address to advertise
            "http://127.0.0.1:14192",
        );
        let protocol = protocol();
        let socket = webrtc::Socket::new(&server_addresses, &protocol.socket);
        let mut server = Server::new(ServerConfig::default(), protocol);
        server.listen(socket);

        // Create a new, singular room, which will contain Users and Entities that they
        // can receive updates from
        let main_room_key = server.make_room().key();

        // Create 4 Character entities, with a range of X and name values
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
                let character = Character::new((count * 4) as u8, 0, first, last);
                let character_key = server
                    .spawn_entity(world.proxy_mut())
                    .insert_component(character)
                    .id();

                // Add the Character Entity to the main Room
                server.room_mut(&main_room_key).add_entity(&character_key);
            }
        }

        App {
            server,
            world,
            main_room_key,
            tick_count: 0,
        }
    }

    pub fn update(&mut self) {
        let mut events = self.server.receive(self.world.proxy_mut());
        if events.is_empty() {
            // If we don't sleep here, app will loop at 100% CPU until a new message comes in
            sleep(Duration::from_millis(3));
            return;
        } else {
            for (user_key, auth) in events.read::<AuthEvent<Auth>>() {
                if auth.username == "charlie" && auth.password == "12345" {
                    // Accept incoming connection
                    self.server.accept_connection(&user_key);
                } else {
                    // Reject incoming connection
                    self.server.reject_connection(&user_key);
                }
            }
            for user_key in events.read::<ConnectEvent>() {
                info!(
                    "Naia Server connected to: {}",
                    self.server.user(&user_key).address()
                );
                self.server
                    .room_mut(&self.main_room_key)
                    .add_user(&user_key);
            }
            for (_user_key, user) in events.read::<DisconnectEvent>() {
                info!("Naia Server disconnected from: {:?}", user.address);
            }
            for (user_key, message) in
                events.read::<MessageEvent<UnorderedReliableChannel, StringMessage>>()
            {
                let message_contents = &(*message.contents);
                info!(
                    "Server recv from ({}) <- {}",
                    self.server.user(&user_key).address(),
                    message_contents
                );
            }
            for _ in events.read::<TickEvent>() {
                // All game logic should happen here, on a tick event

                // Message sending
                for user_key in self.server.user_keys() {
                    let new_message_contents = format!("Server Message ({})", self.tick_count);
                    info!(
                        "Server send to   ({}) -> {}",
                        self.server.user(&user_key).address(),
                        new_message_contents
                    );

                    let new_message = StringMessage::new(new_message_contents);
                    self.server
                        .send_message::<UnorderedReliableChannel, _>(&user_key, &new_message);
                }

                // Iterate through Characters, marching them from (0,0) to (20, N)
                for entity in self.server.entities(self.world.proxy()) {
                    if let Some(mut character) = self
                        .server
                        .entity_mut(self.world.proxy_mut(), &entity)
                        .component::<Character>()
                    {
                        character.step();
                    }
                }

                // Update scopes of entities
                {
                    let server = &mut self.server;
                    let world = &self.world;
                    for (_, user_key, entity) in server.scope_checks() {
                        if let Some(character) = world.proxy().component::<Character>(&entity) {
                            let x = *character.x;
                            if (5..=15).contains(&x) {
                                server.user_scope(&user_key).include(&entity);
                            } else {
                                server.user_scope(&user_key).exclude(&entity);
                            }
                        }
                    }
                }

                // VERY IMPORTANT! Calling this actually sends all update data
                // packets to all Clients that require it. If you don't call this
                // method, the Server will never communicate with it's connected Clients
                self.server.send_all_updates(self.world.proxy());

                self.tick_count = self.tick_count.wrapping_add(1);
            }
            for error in events.read::<ErrorEvent>() {
                info!("Naia Server Error: {}", error);
            }
        }
    }
}
