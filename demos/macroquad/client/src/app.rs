use std::collections::HashSet;

use macroquad::prelude::*;

use naia_client::{Client as NaiaClient, ClientConfig, Event};

use naia_demo_world::{Entity, World as DemoWorld, WorldMutType, WorldRefType};

use naia_macroquad_demo_shared::{
    behavior as shared_behavior, get_server_address, get_shared_config,
    protocol::{Auth, Color, KeyCommand, Protocol, Square},
};

type World = DemoWorld<Protocol>;
type Client = NaiaClient<Protocol, Entity>;

const SQUARE_SIZE: f32 = 32.0;

pub struct App {
    client: Client,
    world: World,
    queued_command: Option<KeyCommand>,
    owned_entity: Option<Entity>,
    squares: HashSet<Entity>,
}

impl App {
    pub fn new() -> Self {
        info!("Naia Macroquad Client Demo started");

        let server_address = get_server_address();
        let auth = Auth::new("charlie", "12345");

        let mut client = Client::new(ClientConfig::default(), get_shared_config());
        client.auth(auth);
        client.connect(server_address);

        App {
            client,
            world: World::new(),
            queued_command: None,
            owned_entity: None,
            squares: HashSet::new(),
        }
    }

    pub fn update(&mut self) {
        self.input();
        self.receive_events();
        self.draw();
    }

    fn input(&mut self) {
        if let Some(entity) = self.owned_entity {
            let entity_net_id = self.client.entity_net_id(&entity);
            let w = is_key_down(KeyCode::W);
            let s = is_key_down(KeyCode::S);
            let a = is_key_down(KeyCode::A);
            let d = is_key_down(KeyCode::D);

            if let Some(command) = &mut self.queued_command {
                if w {
                    command.w.set(true);
                }
                if s {
                    command.s.set(true);
                }
                if a {
                    command.a.set(true);
                }
                if d {
                    command.d.set(true);
                }
            } else {
                self.queued_command = Some(KeyCommand::new(entity_net_id, w, s, a, d));
            }
        }
    }

    fn receive_events(&mut self) {
        for event in self.client.receive(self.world.proxy_mut()) {
            match event {
                Ok(Event::Connection) => {
                    info!("Client connected to: {}", self.client.server_address());
                }
                Ok(Event::Disconnection) => {
                    info!("Client disconnected from: {}", self.client.server_address());
                }
                Ok(Event::Tick) => {
                    if let Some(command) = self.queued_command.take() {
                        let entity_net_id = command.entity_net_id.get();
                        let entity = self.client.entity_from_net_id(entity_net_id);

                        // Send command


                        // Apply command
                        if let Some(mut square_ref) =
                            self.world.proxy_mut().get_component_mut::<Square>(&entity)
                        {
                            shared_behavior::process_command(&command, &mut square_ref);
                        }
                    }
                }
                Ok(Event::SpawnEntity(entity, _)) => {
                    self.squares.insert(entity);
                }
                Ok(Event::DespawnEntity(entity)) => {
                    self.squares.remove(&entity);
                }
                Ok(Event::Message(Protocol::EntityAssignment(entity_assignment))) => {
                    let assign = *entity_assignment.assign.get();
                    let entity_net_id = entity_assignment.entity_net_id.get();

                    let entity = self.client.entity_from_net_id(entity_net_id);

                    if assign {
                        info!("gave ownership of entity");
                        self.owned_entity = Some(entity);
                    } else {
                        let mut disown: bool = false;
                        if let Some(owned_entity) = self.owned_entity {
                            if owned_entity == entity { disown = true; }
                        }
                        if disown {
                            info!("removed ownership of entity");
                            self.owned_entity = None;
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

        if self.client.connected() {
            // draw unowned squares
            for entity in &self.squares {
                if let Some(square) = self.world.proxy().get_component::<Square>(entity) {
                    let color = match square.color.get() {
                        Color::Red => RED,
                        Color::Blue => BLUE,
                        Color::Yellow => YELLOW,
                    };
                    draw_rectangle(
                        f32::from(*(square.x.get())),
                        f32::from(*(square.y.get())),
                        SQUARE_SIZE,
                        SQUARE_SIZE,
                        color,
                    );
                }
            }

            // draw own square
            if let Some(entity) = &self.owned_entity {
                if let Some(square) = self.world.proxy().get_component::<Square>(entity) {
                    draw_rectangle(
                        f32::from(*(square.x.get())),
                        f32::from(*(square.y.get())),
                        SQUARE_SIZE,
                        SQUARE_SIZE,
                        WHITE,
                    );
                }
            }
        }
    }
}
