use std::collections::HashMap;

use macroquad::prelude::*;

use naia_client::{Client, ClientConfig, Event, LocalEntityKey, Ref};

use naia_macroquad_demo_shared::{
    behavior as shared_behavior, get_server_address, get_shared_config,
    protocol::{Auth, Color, KeyCommand, Protocol, Square},
};

const SQUARE_SIZE: f32 = 32.0;

pub struct App {
    client: Client<Protocol>,
    owned_square: Option<(LocalEntityKey, Ref<Square>)>,
    queued_command: Option<Ref<KeyCommand>>,
    square_map: HashMap<LocalEntityKey, Ref<Square>>,
}

impl App {
    pub fn new() -> Self {
        info!("Naia Macroquad Client Demo started");

        let mut client_config = ClientConfig::default();
        client_config.socket_config.server_address = get_server_address();

        // This will be evaluated in the Server's 'on_auth()' method
        let auth = Auth::new("charlie", "12345");

        let client = Client::new(Some(client_config), get_shared_config(), Some(auth));

        App {
            client,
            owned_square: None,
            queued_command: None,
            square_map: HashMap::new(),
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
        for event in self.client.receive() {
            match event {
                Ok(Event::Connection) => {
                    info!("Client connected to: {}", self.client.server_address());
                }
                Ok(Event::Disconnection) => {
                    info!("Client disconnected from: {}", self.client.server_address());
                }
                Ok(Event::Tick) => {
                    if let Some((square_key, _)) = self.owned_square {
                        if let Some(command) = self.queued_command.take() {
                            self.client.queue_command(&square_key, &command);
                        }
                    }
                }
                Ok(Event::SpawnEntity(entity_key, _)) => {
                    if let Some(square_ref) = self.client.entity(&entity_key).component::<Square>()
                    {
                        self.square_map.insert(entity_key, square_ref.clone());
                    }
                }
                Ok(Event::DespawnEntity(entity_key)) => {
                    self.square_map.remove(&entity_key);
                }
                Ok(Event::OwnEntity(entity_key)) => {
                    info!("entity assigned");
                    if let Some(square_ref) = self
                        .client
                        .entity(&entity_key)
                        .prediction()
                        .component::<Square>()
                    {
                        self.owned_square = Some((entity_key, square_ref.clone()));
                    }
                }
                Ok(Event::DisownEntity(_)) => {
                    self.owned_square = None;
                    info!("entity unassigned");
                }
                Ok(Event::NewCommand(_, Protocol::KeyCommand(key_command_ref)))
                | Ok(Event::ReplayCommand(_, Protocol::KeyCommand(key_command_ref))) => {
                    if let Some((_, square_ref)) = &self.owned_square {
                        shared_behavior::process_command(&key_command_ref, &square_ref);
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
            for (_, square_ref) in &self.square_map {
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

            // draw own square
            if let Some((_, square_ref)) = &self.owned_square {
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
