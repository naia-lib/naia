use std::{collections::HashSet, time::Duration};

use macroquad::prelude::*;

use naia_client::{
    shared::{Protocolize, Replicate, Timer},
    Client as NaiaClient, ClientConfig, Event,
};

use naia_demo_world::{Entity, World as DemoWorld, WorldMutType, WorldRefType};

use naia_macroquad_demo_shared::{
    behavior as shared_behavior,
    protocol::{Auth, Color, KeyCommand, Protocol, Square},
    shared_config,
};

use crate::command_history::CommandHistory;

type World = DemoWorld<Protocol>;
type Client = NaiaClient<Protocol, Entity>;

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
    bandwidth_timer: Timer,
}

impl App {
    pub fn new() -> Self {
        info!("Naia Macroquad Client Demo started");

        let mut client_config = ClientConfig::default();

        client_config.connection.bandwidth_measure_duration = Some(Duration::from_secs(1));

        let client = Client::new(&client_config, &shared_config());

        App {
            client,
            world: World::new(),
            owned_entity: None,
            squares: HashSet::new(),
            queued_command: None,
            command_history: CommandHistory::new(),
            bandwidth_timer: Timer::new(Duration::from_secs(1)),
        }
    }

    pub fn update(&mut self) {
        let q = is_key_pressed(KeyCode::Q);
        let c = is_key_pressed(KeyCode::C);
        if q {
            if self.client.is_connected() {
                return self.client.disconnect();
            }
        }
        if c {
            if self.client.is_disconnected() {
                let auth = Auth::new("charlie", "12345");
                self.client.auth(auth);
                return self.client.connect("http://127.0.0.1:14191");
            }
        }

        if self.client.is_connected() {
            if self.bandwidth_timer.ringing() {
                self.bandwidth_timer.reset();

                info!(
                    "Bandwidth: {} kbps incoming, {} kbps outgoing",
                    self.client.incoming_bandwidth(),
                    self.client.outgoing_bandwidth()
                );
            }
        }

        self.input();
        self.receive_events();
        self.draw();
    }

    fn input(&mut self) {
        if let Some(_) = self.owned_entity {
            let w = is_key_down(KeyCode::W);
            let s = is_key_down(KeyCode::S);
            let a = is_key_down(KeyCode::A);
            let d = is_key_down(KeyCode::D);

            if w || s || a || d {
                if let Some(command) = &mut self.queued_command {
                    if w {
                        *command.w = true;
                    }
                    if s {
                        *command.s = true;
                    }
                    if a {
                        *command.a = true;
                    }
                    if d {
                        *command.d = true;
                    }
                } else {
                    self.queued_command = Some(KeyCommand::new(w, s, a, d));
                }
            }
        }
    }

    fn receive_events(&mut self) {
        if self.client.is_disconnected() {
            return;
        }
        for event in self.client.receive(self.world.proxy_mut()) {
            match event {
                Ok(Event::Connection(server_address)) => {
                    info!("Client connected to: {}", server_address);
                }
                Ok(Event::Disconnection(server_address)) => {
                    info!("Client disconnected from: {}", server_address);

                    self.world = World::new();
                    self.owned_entity = None;
                    self.squares = HashSet::new();
                    self.queued_command = None;
                    self.command_history = CommandHistory::new();
                }
                Ok(Event::Tick) => {
                    if let Some(owned_entity) = &self.owned_entity {
                        if let Some(command) = self.queued_command.take() {
                            if let Some(client_tick) = self.client.client_tick() {
                                // Record command
                                self.command_history
                                    .push_front(client_tick, command.clone());

                                // Send command
                                self.client
                                    .entity_mut(&owned_entity.confirmed)
                                    .send_message(&command);

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
                Ok(Event::SpawnEntity(entity, _)) => {
                    self.squares.insert(entity);
                }
                Ok(Event::DespawnEntity(entity)) => {
                    self.squares.remove(&entity);
                }
                Ok(Event::MessageEntity(entity, Protocol::EntityAssignment(entity_assignment))) => {
                    let assign = *entity_assignment.assign;

                    if assign {
                        info!("gave ownership of entity");

                        ////////////////////////////////
                        let mut world_mut = self.world.proxy_mut();
                        let prediction_entity = world_mut.spawn_entity();

                        // create copies of components //
                        for component_kind in world_mut.component_kinds(&entity) {
                            let mut component_copy_opt: Option<Protocol> = None;
                            if let Some(component) =
                                world_mut.component_of_kind(&entity, &component_kind)
                            {
                                component_copy_opt = Some(component.clone());
                            }
                            if let Some(component_copy) = component_copy_opt {
                                component_copy
                                    .extract_and_insert(&prediction_entity, &mut world_mut);
                            }
                        }
                        ////////////////////////////////

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
                Ok(Event::UpdateComponent(server_tick, updated_entity, _)) => {
                    if let Some(owned_entity) = &self.owned_entity {
                        let mut world_mut = self.world.proxy_mut();
                        let server_entity = owned_entity.confirmed;

                        // If entity is owned
                        if updated_entity == server_entity {
                            let client_entity = owned_entity.predicted;

                            // Set prediction to server authoritative Entity
                            // go through all components to make prediction components = world
                            // components
                            for component_kind in world_mut.component_kinds(&server_entity) {
                                world_mut.mirror_components(
                                    &client_entity,
                                    &server_entity,
                                    &component_kind,
                                );
                            }

                            // Remove history of commands until current received tick
                            self.command_history.remove_to_and_including(server_tick);

                            // Replay all existing historical commands until current tick
                            let mut command_iter = self.command_history.iter_mut();
                            while let Some((_, command)) = command_iter.next() {
                                if let Some(mut square_ref) =
                                    world_mut.component_mut::<Square>(&client_entity)
                                {
                                    shared_behavior::process_command(&command, &mut square_ref);
                                }
                            }
                        }
                    }
                }
                Err(err) => {
                    info!("Client Error: {}", err);
                }
                _ => {}
            }
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
