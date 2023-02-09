use std::collections::HashSet;

use macroquad::prelude::{
    clear_background, draw_rectangle, info, is_key_down, KeyCode, BLACK, BLUE, GREEN, RED, WHITE,
    YELLOW,
};

use naia_client::{
    Client as NaiaClient, ClientConfig, CommandHistory, ConnectionEvent, DespawnEntityEvent,
    DisconnectionEvent, ErrorEvent, InsertComponentEvent, MessageEvent, RemoveComponentEvent,
    SpawnEntityEvent, TickEvent, UpdateComponentEvent,
};

use naia_demo_world::{Entity, World, WorldMutType, WorldRefType};

use naia_macroquad_demo_shared::{
    behavior as shared_behavior,
    channels::{EntityAssignmentChannel, PlayerCommandChannel},
    components::{Color, Square},
    messages::{Auth, EntityAssignment, KeyCommand},
    protocol,
};

type Client = NaiaClient<Entity>;

const SQUARE_SIZE: f32 = 32.0;

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
    squares: HashSet<Entity>,
    queued_command: Option<KeyCommand>,
    command_history: CommandHistory<KeyCommand>,
}

impl App {
    pub fn new() -> Self {
        info!("Naia Macroquad Client Demo started");

        let mut client = Client::new(ClientConfig::default(), protocol());
        client.auth(Auth::new("charlie", "12345"));
        client.connect("http://127.0.0.1:14191");

        App {
            client,
            world: World::default(),
            owned_entity: None,
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
    }

    fn receive_events(&mut self) {
        if self.client.is_disconnected() {
            return;
        }

        let mut events = self.client.receive(self.world.proxy_mut());

        for server_address in events.read::<ConnectionEvent>() {
            info!("Client connected to: {}", server_address);
        }
        for server_address in events.read::<DisconnectionEvent>() {
            info!("Client disconnected from: {}", server_address);

            self.world = World::default();
            self.owned_entity = None;
            self.squares = HashSet::new();
            self.queued_command = None;
            self.command_history = CommandHistory::default();
        }
        for _ in events.read::<TickEvent>() {
            if let Some(owned_entity) = &self.owned_entity {
                if let Some(command) = self.queued_command.take() {
                    if let Some(client_tick) = self.client.client_tick() {
                        if self.command_history.can_insert(&client_tick) {
                            // Record command
                            self.command_history.insert(client_tick, command.clone());

                            // Send command
                            self.client
                                .send_message::<PlayerCommandChannel, _>(&command);

                            // Apply command
                            if let Some(mut square_ref) = self
                                .world
                                .proxy_mut()
                                .component_mut::<Square>(&owned_entity.predicted)
                            {
                                shared_behavior::process_command(&command, &mut square_ref);
                            }
                        }
                    }
                }
            }
        }
        for entity in events.read::<SpawnEntityEvent>() {
            self.squares.insert(entity);
            info!("spawned entity");
        }
        for entity in events.read::<DespawnEntityEvent>() {
            self.squares.remove(&entity);
            info!("despawned entity");
            // TODO: Sync up Predicted & Confirmed entities
        }
        for (_entity, _component_kind) in events.read::<InsertComponentEvent>() {
            info!("inserted component");
            // TODO: Sync up Predicted & Confirmed entities
        }
        for (_entity, _component) in events.read::<RemoveComponentEvent<Square>>() {
            info!("removed component");
            // TODO: Sync up Predicted & Confirmed entities
        }
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
        for (server_tick, updated_entity, _updated_component_type) in
            events.read::<UpdateComponentEvent>()
        {
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
                        if let Some(mut square_ref) = self
                            .world
                            .proxy_mut()
                            .component_mut::<Square>(&client_entity)
                        {
                            shared_behavior::process_command(&command, &mut square_ref);
                        }
                    }
                }
            }
        }
        for error in events.read::<ErrorEvent>() {
            info!("Client Error: {}", error);
        }
    }

    fn draw(&mut self) {
        clear_background(BLACK);

        if self.client.is_connected() {
            // draw unowned squares
            for entity in &self.squares {
                if let Some(square) = self.world.proxy().component::<Square>(entity) {
                    let color = match *square.color {
                        Color::Red => RED,
                        Color::Blue => BLUE,
                        Color::Yellow => YELLOW,
                        Color::Green => GREEN,
                    };
                    draw_rectangle(
                        f32::from(*square.x),
                        f32::from(*square.y),
                        SQUARE_SIZE,
                        SQUARE_SIZE,
                        color,
                    );
                }
            }

            // draw own square
            if let Some(entity) = &self.owned_entity {
                if let Some(square) = self.world.proxy().component::<Square>(&entity.predicted) {
                    draw_rectangle(
                        f32::from(*square.x),
                        f32::from(*square.y),
                        SQUARE_SIZE,
                        SQUARE_SIZE,
                        WHITE,
                    );
                }
            }
        }
    }
}
