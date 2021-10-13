use std::collections::HashSet;

use macroquad::prelude::*;

use naia_client::{Client as NaiaClient, ClientConfig, Event, Ref};

use naia_default_world::{Entity, World as DefaultWorld, WorldRefType};

use naia_macroquad_demo_shared::{
    behavior as shared_behavior, get_server_address, get_shared_config,
    protocol::{Auth, Color, KeyCommand, Protocol, Square},
};

type World = DefaultWorld<Protocol>;
type Client = NaiaClient<Protocol, Entity>;

const SQUARE_SIZE: f32 = 32.0;

pub struct App {
    client: Client,
    world: World,
    queued_command: Option<Ref<KeyCommand>>,
    owned_entity: Option<Entity>,
    squares: HashSet<Entity>,
}

impl App {
    pub fn new() -> Self {
        info!("Naia Macroquad Client Demo started");

        let server_address = get_server_address();
        let auth = Some(Auth::new("charlie", "12345"));

        let mut client = Client::new(ClientConfig::default(), get_shared_config());
        client.connect(server_address, auth);

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
        let w = is_key_down(KeyCode::W);
        let s = is_key_down(KeyCode::S);
        let a = is_key_down(KeyCode::A);
        let d = is_key_down(KeyCode::D);

        if let Some(command_ref) = &mut self.queued_command {
            let mut command = command_ref.borrow_mut();
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
            self.queued_command = Some(KeyCommand::new(w, s, a, d));
        }
    }

    fn receive_events(&mut self) {
        for event in self.client.receive(&mut self.world.proxy_mut()) {
            match event {
                Ok(Event::Connection) => {
                    info!("Client connected to: {}", self.client.server_address());
                }
                Ok(Event::Disconnection) => {
                    info!("Client disconnected from: {}", self.client.server_address());
                }
                Ok(Event::Tick) => {
                    if let Some(entity) = self.owned_entity {
                        if let Some(command) = self.queued_command.take() {
                            self.client.queue_command(&entity, &command);
                        }
                    }
                }
                Ok(Event::SpawnEntity(entity, _)) => {
                    self.squares.insert(entity);
                }
                Ok(Event::DespawnEntity(entity)) => {
                    self.squares.remove(&entity);
                }
                Ok(Event::OwnEntity(entity)) => {
                    info!("gave ownership of entity");
                    self.owned_entity = Some(entity.predicted);
                }
                Ok(Event::DisownEntity(_)) => {
                    info!("removed ownership of entity");
                    self.owned_entity = None;
                }
                Ok(Event::NewCommand(_, Protocol::KeyCommand(key_command_ref)))
                | Ok(Event::ReplayCommand(_, Protocol::KeyCommand(key_command_ref))) => {
                    if let Some(entity) = &self.owned_entity {
                        if let Some(square_ref) = self.world.proxy().get_component::<Square>(entity) {
                            shared_behavior::process_command(&key_command_ref, &square_ref);
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
                if let Some(square_ref) = self.world.proxy().get_component::<Square>(entity) {
                    let square = square_ref.borrow();
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
                if let Some(square_ref) = self.world.proxy().get_component::<Square>(entity) {
                    let square = square_ref.borrow();
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
