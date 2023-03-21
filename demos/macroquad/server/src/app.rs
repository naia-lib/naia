use std::{collections::HashMap, thread::sleep, time::Duration};

use naia_server::{
    shared::Random, transport::webrtc, AuthEvent, ConnectEvent, DisconnectEvent, ErrorEvent,
    InsertComponentEvent, RoomKey, Server as NaiaServer, ServerConfig, TickEvent,
    UpdateComponentEvent, UserKey,
};

use naia_demo_world::{Entity, World, WorldMutType, WorldRefType};

use naia_macroquad_demo_shared::{
    behavior as shared_behavior,
    channels::{EntityAssignmentChannel, PlayerCommandChannel},
    components::{Color, ColorValue, Position, Shape, ShapeValue},
    messages::{Auth, EntityAssignment, KeyCommand},
    protocol,
};

type Server = NaiaServer<Entity>;

pub struct App {
    server: Server,
    world: World,
    main_room_key: RoomKey,
    user_to_square_map: HashMap<UserKey, Entity>,
    user_to_cursor_map: HashMap<UserKey, Entity>,
    square_last_command: HashMap<Entity, KeyCommand>,
}

impl App {
    pub fn new() -> Self {
        info!("Naia Macroquad Server Demo started");

        let protocol = protocol();

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

        let socket = webrtc::Socket::new(&server_addresses, &protocol.socket);

        let mut server = Server::new(ServerConfig::default(), protocol);
        server.listen(socket);

        // Create a new, singular room, which will contain Users and Entities that they
        // can receive updates from
        let main_room_key = server.make_room().key();

        App {
            server,
            world: World::default(),
            main_room_key,
            user_to_square_map: HashMap::new(),
            user_to_cursor_map: HashMap::new(),
            square_last_command: HashMap::new(),
        }
    }

    pub fn update(&mut self) {
        let mut events = self.server.receive(self.world.proxy_mut());
        if events.is_empty() {
            // If we don't sleep here, app will loop at 100% CPU until a new message comes in
            sleep(Duration::from_millis(3));
            return;
        }

        // Auth Events
        for (user_key, auth) in events.read::<AuthEvent<Auth>>() {
            if auth.username == "charlie" && auth.password == "12345" {
                // Accept incoming connection
                self.server.accept_connection(&user_key);
            } else {
                // Reject incoming connection
                self.server.reject_connection(&user_key);
            }
        }

        // Connect Events
        for user_key in events.read::<ConnectEvent>() {
            // New User has joined the Server
            let user_address = self
                .server
                .user_mut(&user_key)
                // User enters a Room to see the contained Entities
                .enter_room(&self.main_room_key)
                .address();

            info!("Naia Server connected to: {}", user_address);

            // Create Position Component
            let x = (Random::gen_range_u32(0, 50) * 16) as i16;
            let y = (Random::gen_range_u32(0, 37) * 16) as i16;
            let position_component = Position::new(x, y);

            // Create Color Component
            let color_value = match self.server.users_count() % 4 {
                0 => ColorValue::Yellow,
                1 => ColorValue::Red,
                2 => ColorValue::Blue,
                _ => ColorValue::Green,
            };
            let color_component = Color::new(color_value);

            // Spawn new Square Entity
            let entity_id = self
                .server
                .spawn_entity(self.world.proxy_mut())
                // Entity enters Room
                .enter_room(&self.main_room_key)
                // Add Position component to Entity
                .insert_component(position_component)
                // Add Color component to Entity
                .insert_component(color_component)
                // Add Shape component to Entity
                .insert_component(Shape::new(ShapeValue::Square))
                // Get Entity ID
                .id();

            // Associate new Entity with User that spawned it
            self.user_to_square_map.insert(user_key, entity_id);
            self.square_last_command
                .insert(entity_id, KeyCommand::new(false, false, false, false));

            // Send an Entity Assignment message to the User that owns the Square
            let mut assignment_message = EntityAssignment::new(true);
            assignment_message.entity.set(&self.server, &entity_id);

            // TODO: eventually would like to do this like:
            // self.server.entity_property(assigment_message).set(&entity_id);

            self.server
                .send_message::<EntityAssignmentChannel, _>(&user_key, &assignment_message);
        }

        // Disconnect Events
        for (user_key, user) in events.read::<DisconnectEvent>() {
            info!("Naia Server disconnected from: {}", user.address);
            if let Some(entity) = self.user_to_square_map.remove(&user_key) {
                self.server
                    .entity_mut(self.world.proxy_mut(), &entity)
                    .leave_room(&self.main_room_key)
                    .despawn();
                self.square_last_command.remove(&entity);
            }
            if let Some(entity) = self.user_to_cursor_map.remove(&user_key) {
                self.server
                    .entity_mut(self.world.proxy_mut(), &entity)
                    .leave_room(&self.main_room_key)
                    .despawn();
            }
        }

        // Insert Component Events for Client Cursors
        for (user_key, client_cursor_entity) in events.read::<InsertComponentEvent<Position>>() {
            info!("insert component into client entity");
            let (client_cursor_position_x, client_cursor_position_y) = {
                if let Some(client_cursor_position) = self
                    .world
                    .proxy()
                    .component::<Position>(&client_cursor_entity)
                {
                    (*client_cursor_position.x, *client_cursor_position.y)
                } else {
                    continue;
                }
            };

            // Create Position Component
            let server_cursor_position =
                Position::new(client_cursor_position_x, client_cursor_position_y);

            // Create Color Component
            let color_value = match self.server.users_count() % 4 {
                0 => ColorValue::Yellow,
                1 => ColorValue::Red,
                2 => ColorValue::Blue,
                _ => ColorValue::Green,
            };
            let server_cursor_color = Color::new(color_value);

            // Spawn new Cursor Entity
            let server_cursor_entity = self
                .server
                .spawn_entity(self.world.proxy_mut())
                // Entity enters Room
                .enter_room(&self.main_room_key)
                // Add Position component to Entity
                .insert_component(server_cursor_position)
                // Add Color component to Entity
                .insert_component(server_cursor_color)
                // Add Shape component to Entity
                .insert_component(Shape::new(ShapeValue::Circle))
                // Get Entity ID
                .id();

            self.user_to_cursor_map
                .insert(user_key, server_cursor_entity);
        }

        // Update Component Events for Client Cursors
        for (user_key, client_cursor_entity) in events.read::<UpdateComponentEvent<Position>>() {
            let (client_cursor_position_x, client_cursor_position_y) = {
                if let Some(client_cursor_position) = self
                    .world
                    .proxy()
                    .component::<Position>(&client_cursor_entity)
                {
                    (*client_cursor_position.x, *client_cursor_position.y)
                } else {
                    continue;
                }
            };

            if let Some(server_cursor_entity) = self.user_to_cursor_map.get(&user_key) {
                if let Some(mut server_cursor_position) = self
                    .world
                    .proxy_mut()
                    .component_mut::<Position>(server_cursor_entity)
                {
                    *server_cursor_position.x = client_cursor_position_x;
                    *server_cursor_position.y = client_cursor_position_y;
                }
            }
        }

        // Tick Events
        let mut has_ticked = false;

        for server_tick in events.read::<TickEvent>() {
            has_ticked = true;

            // All game logic should happen here, on a tick event

            let mut messages = self.server.receive_tick_buffer_messages(&server_tick);
            for (_user_key, key_command) in messages.read::<PlayerCommandChannel, KeyCommand>() {
                let Some(entity) = &key_command.entity.get(&self.server) else {
                    continue;
                };
                if let Some(mut position) = self
                    .server
                    .entity_mut(self.world.proxy_mut(), &entity)
                    .component::<Position>()
                {
                    shared_behavior::process_command(&key_command, &mut position);
                }
            }
        }

        if has_ticked {
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

        // Error Events
        for error in events.read::<ErrorEvent>() {
            info!("Naia Server error: {}", error);
        }
    }
}
