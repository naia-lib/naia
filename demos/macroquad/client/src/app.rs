use std::collections::HashSet;

use macroquad::prelude::{
    clear_background, draw_circle, draw_rectangle, info, is_key_down, KeyCode, BLACK, BLUE, GREEN,
    RED, WHITE, YELLOW,
};

use naia_client::{
    transport::webrtc, Client as NaiaClient, ClientConfig, ClientTickEvent, CommandHistory,
    ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent, MessageEvent, SpawnEntityEvent,
    UpdateComponentEvent,
};

use naia_demo_world::{Entity, World, WorldMutType, WorldRefType};

use naia_macroquad_demo_shared::{
    behavior as shared_behavior,
    channels::{EntityAssignmentChannel, PlayerCommandChannel},
    components::{Color, ColorValue, Position, Shape, ShapeValue},
    messages::{Auth, EntityAssignment, KeyCommand},
    protocol,
};

type Client = NaiaClient<Entity>;

const SQUARE_SIZE: f32 = 32.0;
const CIRCLE_RADIUS: f32 = 6.0;

struct OwnedEntity {
    pub confirmed: Entity,
    pub predicted: Entity,
}

impl OwnedEntity {
    pub fn new(confirmed_entity: Entity, predicted_entity: Entity) -> Self {
        OwnedEntity {
            confirmed: confirmed_entity,
            predicted: predicted_entity,
        }
    }
}

pub struct App {
    client: Client,
    world: World,
    owned_entity: Option<OwnedEntity>,
    cursor_entity: Option<Entity>,
    squares: HashSet<Entity>,
    queued_command: Option<KeyCommand>,
    command_history: CommandHistory<KeyCommand>,
}

impl App {
    pub fn new() -> Self {
        info!("Naia Macroquad Client Demo started");

        let protocol = protocol();
        let socket = webrtc::Socket::new("http://127.0.0.1:14191", &protocol.socket);
        let mut client = Client::new(ClientConfig::default(), protocol);
        client.auth(Auth::new("charlie", "12345"));
        client.connect(socket);

        App {
            client,
            world: World::default(),
            owned_entity: None,
            cursor_entity: None,
            squares: HashSet::new(),
            queued_command: None,
            command_history: CommandHistory::default(),
        }
    }

    pub fn update(&mut self) {
        self.input();
        self.receive_events();
        self.draw();
    }

    fn input(&mut self) {
        // Keyboard events
        if let Some(owned_entity) = &self.owned_entity {
            let w = is_key_down(KeyCode::W);
            let s = is_key_down(KeyCode::S);
            let a = is_key_down(KeyCode::A);
            let d = is_key_down(KeyCode::D);

            if w || s || a || d {
                if let Some(command) = &mut self.queued_command {
                    if w {
                        command.w = true;
                    }
                    if s {
                        command.s = true;
                    }
                    if a {
                        command.a = true;
                    }
                    if d {
                        command.d = true;
                    }
                } else {
                    let mut key_command = KeyCommand::new(w, s, a, d);
                    key_command
                        .entity
                        .set(&self.client, &owned_entity.confirmed);
                    self.queued_command = Some(key_command);
                }
            }
        }

        // Cursor events
        if let Some(cursor_entity) = &self.cursor_entity {
            if let Some(mut cursor_position) = self
                .world
                .proxy_mut()
                .component_mut::<Position>(cursor_entity)
            {
                *cursor_position.x = macroquad::input::mouse_position().0 as i16;
                *cursor_position.y = macroquad::input::mouse_position().1 as i16;
            }
        }
    }

    fn receive_events(&mut self) {
        if self.client.is_disconnected() {
            return;
        }

        let mut events = self.client.receive(self.world.proxy_mut());

        // Connect Events
        for server_address in events.read::<ConnectEvent>() {
            info!("Client connected to: {}", server_address);

            // Create Position Component
            let x = 0;
            let y = 0;
            let position_component = Position::new(x, y);

            // Spawn new Client-authoritative Cursor entity
            let entity_id = self
                .client
                .spawn_entity(self.world.proxy_mut())
                // Add Position component to Entity
                .insert_component(position_component)
                // Get Entity ID
                .id();

            self.cursor_entity = Some(entity_id);
        }

        // Disconnect Events
        for server_address in events.read::<DisconnectEvent>() {
            info!("Client disconnected from: {}", server_address);

            self.world = World::default();
            self.owned_entity = None;
            self.squares = HashSet::new();
            self.queued_command = None;
            self.command_history = CommandHistory::default();
        }

        // Message Events
        for entity_assignment in
            events.read::<MessageEvent<EntityAssignmentChannel, EntityAssignment>>()
        {
            let assign = entity_assignment.assign;

            let entity = entity_assignment.entity.get(&self.client).unwrap();
            if assign {
                info!("gave ownership of entity");
                let prediction_entity = self.world.proxy_mut().duplicate_entity(&entity);
                self.owned_entity = Some(OwnedEntity::new(entity, prediction_entity));
            } else {
                let mut disowned: bool = false;
                if let Some(owned_entity) = &self.owned_entity {
                    if owned_entity.confirmed == entity {
                        self.world
                            .proxy_mut()
                            .despawn_entity(&owned_entity.predicted);
                        disowned = true;
                    }
                }
                if disowned {
                    info!("removed ownership of entity");
                    self.owned_entity = None;
                }
            }
        }

        // Client Tick Events
        for client_tick in events.read::<ClientTickEvent>() {
            let Some(owned_entity) = &self.owned_entity else {
                continue;
            };
            let Some(command) = self.queued_command.take() else {
                continue;
            };
            if self.command_history.can_insert(&client_tick) {
                // Record command
                self.command_history.insert(client_tick, command.clone());

                // Send command
                self.client
                    .send_tick_buffer_message::<PlayerCommandChannel, _>(&client_tick, &command);

                // Apply command
                if let Some(mut position) = self
                    .world
                    .proxy_mut()
                    .component_mut::<Position>(&owned_entity.predicted)
                {
                    shared_behavior::process_command(&command, &mut position);
                }
            }
        }

        // Spawn Entity Events
        for entity in events.read::<SpawnEntityEvent>() {
            self.squares.insert(entity);
            info!("spawned entity");
        }

        // Despawn Entity Events
        for entity in events.read::<DespawnEntityEvent>() {
            self.squares.remove(&entity);
            info!("despawned entity");
            // TODO: Sync up Predicted & Confirmed entities
        }

        // Update Component Events
        for (server_tick, updated_entity) in events.read::<UpdateComponentEvent<Position>>() {
            if let Some(owned_entity) = &self.owned_entity {
                let server_entity = owned_entity.confirmed;

                // If entity is owned
                if updated_entity == server_entity {
                    let client_entity = owned_entity.predicted;

                    // Set state of all components on Predicted & Confirmed entities to the authoritative Server state
                    self.world
                        .proxy_mut()
                        .mirror_entities(&client_entity, &server_entity);

                    let replay_commands = self.command_history.replays(&server_tick);
                    for (_, command) in replay_commands {
                        if let Some(mut position) = self
                            .world
                            .proxy_mut()
                            .component_mut::<Position>(&client_entity)
                        {
                            shared_behavior::process_command(&command, &mut position);
                        }
                    }
                }
            }
        }

        // Error Events
        for error in events.read::<ErrorEvent>() {
            info!("Client Error: {}", error);
        }
    }

    fn draw(&mut self) {
        clear_background(BLACK);

        if !self.client.is_connected() {
            return;
        }

        // draw unowned squares
        for entity in &self.squares {
            let shape_value = {
                if let Some(shape) = self.world.proxy().component::<Shape>(entity) {
                    (*shape.value).clone()
                } else {
                    continue;
                }
            };
            let color_value = {
                if let Some(color) = self.world.proxy().component::<Color>(entity) {
                    (*color.value).clone()
                } else {
                    continue;
                }
            };
            if let Some(position) = self.world.proxy().component::<Position>(entity) {
                let color_actual = match color_value {
                    ColorValue::Red => RED,
                    ColorValue::Blue => BLUE,
                    ColorValue::Yellow => YELLOW,
                    ColorValue::Green => GREEN,
                };
                match shape_value {
                    ShapeValue::Square => {
                        draw_rectangle(
                            f32::from(*position.x),
                            f32::from(*position.y),
                            SQUARE_SIZE,
                            SQUARE_SIZE,
                            color_actual,
                        );
                    }
                    ShapeValue::Circle => {
                        draw_circle(
                            f32::from(*position.x),
                            f32::from(*position.y),
                            CIRCLE_RADIUS,
                            color_actual,
                        );
                    }
                }
            }
        }

        // draw own (predicted) square
        if let Some(entity) = &self.owned_entity {
            if let Some(position) = self.world.proxy().component::<Position>(&entity.predicted) {
                draw_rectangle(
                    f32::from(*position.x),
                    f32::from(*position.y),
                    SQUARE_SIZE,
                    SQUARE_SIZE,
                    WHITE,
                );
            }
        }

        // draw own client-authoritative cursor
        if let Some(entity) = &self.cursor_entity {
            if let Some(position) = self.world.proxy().component::<Position>(entity) {
                draw_circle(
                    f32::from(*position.x),
                    f32::from(*position.y),
                    CIRCLE_RADIUS,
                    WHITE,
                );
            }
        }
    }
}
