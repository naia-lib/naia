use std::{collections::HashMap, time::Duration};

use macroquad::prelude::*;

use naia_client::{Client, ClientConfig, Event, LocalReplicaKey, Ref};

use naia_macroquad_demo_shared::{
    behavior as shared_behavior, get_server_address, get_shared_config,
    protocol::{Auth, Color, KeyCommand, Protocol, Square},
};

const SQUARE_SIZE: f32 = 32.0;

pub struct App {
    client: Client<Protocol>,
    pawn: Option<(LocalReplicaKey, Ref<Square>)>,
    queued_command: Option<Ref<KeyCommand>>,
    square_map: HashMap<LocalReplicaKey, Ref<Square>>,
}

impl App {
    pub fn new() -> Self {
        info!("Naia Macroquad Client Demo started");

        let mut client_config = ClientConfig::default();
        client_config.socket_config.server_address = get_server_address();
        client_config.heartbeat_interval = Duration::from_secs(2);
        // Keep in mind that the disconnect timeout duration should always be at least
        // 2x greater than the server's heartbeat interval, to make it so that at the
        // worst case, the client would need to miss 2 server heartbeats before
        // disconnecting from them
        client_config.disconnection_timeout_duration = Duration::from_secs(5);

        // This will be evaluated in the Server's 'on_auth()' method
        let auth = Auth::new("charlie", "12345");

        App {
            client: Client::new(
                Protocol::load(),
                Some(client_config),
                get_shared_config(),
                Some(auth),
            ),
            pawn: None,
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
                    if let Some((pawn_key, _)) = self.pawn {
                        if let Some(command) = self.queued_command.take() {
                            self.client.send_object_command(&pawn_key, &command);
                        }
                    }
                }
                Ok(Event::AssignPawn(object_key)) => {
                    info!("assign pawn");
                    if let Some(Protocol::Square(square_ref)) =
                        self.client.get_pawn_mut(&object_key)
                    {
                        self.pawn = Some((object_key, square_ref.clone()));
                    }
                }
                Ok(Event::UnassignPawn(_)) => {
                    self.pawn = None;
                    info!("unassign pawn");
                }
                Ok(Event::NewCommand(_, Protocol::KeyCommand(key_command_ref)))
                | Ok(Event::ReplayCommand(_, Protocol::KeyCommand(key_command_ref))) => {
                    if let Some((_, pawn_ref)) = &self.pawn {
                        shared_behavior::process_command(&key_command_ref, &pawn_ref);
                    }
                }
                Ok(Event::CreateObject(object_key)) => {
                    if let Some(Protocol::Square(square_ref)) = self.client.get_object(&object_key)
                    {
                        self.square_map.insert(object_key, square_ref.clone());
                    }
                }
                Ok(Event::DeleteObject(object_key, _)) => {
                    self.square_map.remove(&object_key);
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

        if self.client.has_connection() {
            // draw squares
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

            // draw pawn
            if let Some((_, pawn_ref)) = &self.pawn {
                let square = pawn_ref.borrow();
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
