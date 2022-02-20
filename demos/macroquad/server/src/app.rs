use std::{time::Duration, collections::HashMap};

use naia_server::{
    shared::Random, Event, RoomKey, Server as NaiaServer, ServerAddrs, ServerConfig, UserKey, shared::Timer
};

use naia_demo_world::{Entity, World as DemoWorld};

use naia_macroquad_demo_shared::{
    behavior as shared_behavior,
    protocol::{Color, EntityAssignment, Protocol, Square},
    shared_config,
};

type World = DemoWorld<Protocol>;
type Server = NaiaServer<Protocol, Entity>;

pub struct App {
    server: Server,
    world: World,
    main_room_key: RoomKey,
    user_to_prediction_map: HashMap<UserKey, Entity>,
    bandwidth_timer: Timer,
}

impl App {
    pub fn new() -> Self {
        info!("Naia Macroquad Server Demo started");

        let server_addresses = ServerAddrs::new(
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

        let mut server_config = ServerConfig::default();

        server_config.connection.bandwidth_measure_duration = Some(Duration::from_secs(1));

        let mut server = Server::new(server_config, shared_config());
        server.listen(server_addresses);

        let world = World::new();

        // Create a new, singular room, which will contain Users and Entities that they
        // can receive updates from
        let main_room_key = server.make_room().key();

        App {
            server,
            world,
            main_room_key,
            user_to_prediction_map: HashMap::<UserKey, Entity>::new(),
            bandwidth_timer: Timer::new(Duration::from_secs(1)),
        }
    }

    pub fn update(&mut self) {

        if self.bandwidth_timer.ringing() {
            self.bandwidth_timer.reset();

            info!("Bandwidth: {} kbps down, {} kbps up", self.server.download_bandwidth_total(), self.server.upload_bandwidth_total());
        }

        for event in self.server.receive() {
            match event {
                Ok(Event::Authorization(user_key, Protocol::Auth(auth))) => {
                    let username = auth.username.get();
                    let password = auth.password.get();
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
                    let entity = self
                        .server
                        .spawn_entity(self.world.proxy_mut())
                        .insert_component(square)
                        .enter_room(&self.main_room_key)
                        .send_message(&user_key, &EntityAssignment::new(true))
                        .id();
                    self.user_to_prediction_map.insert(user_key, entity);
                }
                Ok(Event::Disconnection(user_key, user)) => {
                    info!("Naia Server disconnected from: {}", user.address);
                    if let Some(entity) = self.user_to_prediction_map.remove(&user_key) {
                        self.server
                            .entity_mut(self.world.proxy_mut(), &entity)
                            .leave_room(&self.main_room_key)
                            .despawn();
                    }
                }
                Ok(Event::MessageEntity(_, entity, Protocol::KeyCommand(key_command))) => {
                    if let Some(mut square) = self
                        .server
                        .entity_mut(self.world.proxy_mut(), &entity)
                        .component::<Square>()
                    {
                        shared_behavior::process_command(&key_command, &mut square);
                    }
                }
                Ok(Event::Tick) => {
                    // All game logic should happen here, on a tick event

                    // Check whether Entities are in/out of all possible Scopes
                    for (_, user_key, entity) in self.server.scope_checks() {
                        // You'd normally do whatever checks you need to in here..
                        // to determine whether each Entity should be in scope or not.

                        // This indicates the Entity should be in this scope.
                        self.server.user_scope(&user_key).include(&entity);

                        // And call this if Entity should NOT be in this scope.
                        // self.server.user_scope(..).exclude(..);
                    }

                    // VERY IMPORTANT! Calling this actually sends all update data
                    // packets to all Clients that require it. If you don't call this
                    // method, the Server will never communicate with it's connected Clients
                    self.server.send_all_updates(self.world.proxy());
                }
                Err(error) => {
                    info!("Naia Server error: {}", error);
                }
                _ => {}
            }
        }
    }
}
