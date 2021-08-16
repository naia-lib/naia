use std::{collections::HashMap, time::Duration};

use macroquad::prelude::*;

use naia_client::{Client, ClientConfig, Event, LocalReplicaKey, Ref, Replicate};

use naia_macroquad_demo_shared::{
    behavior as shared_behavior, get_shared_config, get_server_address,
    protocol::{Auth, Color, KeyCommand, Protocol, Square},
};

pub struct App {
    client: Client<Protocol>,
    pawn: Option<(LocalReplicaKey, Ref<Square>)>,
    queued_command: Option<KeyCommand>,
    square_map: HashMap<LocalReplicaKey, Ref<Square>>,
}

impl App {
    pub fn new() -> Self {

        info!("Naia Macroquad Client Demo started");

        let mut client_config = ClientConfig::default();

        client_config.server_address = get_server_address();
        client_config.heartbeat_interval = Duration::from_secs(2);
        client_config.disconnection_timeout_duration = Duration::from_secs(5);

        let auth = Auth::new("charlie", "12345").to_protocol();

        let client = Client::new(
            Protocol::load(),
            Some(client_config),
            get_shared_config(),
            Some(auth),
        );

        App {
            client,
            pawn: None,
            queued_command: None,
            square_map: HashMap::new(),
        }
    }

    pub fn update(&mut self) {
        // input
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
            self.queued_command = Some(KeyCommand::new(w, s, a, d));
        }

        // update
        loop {
            if let Some(result) = self.client.receive() {
                match result {
                    Ok(event) => match event {
                        Event::Connection => {
                            info!("Client connected to: {}", self.client.server_address());
                        }
                        Event::Disconnection => {
                            info!("Client disconnected from: {}", self.client.server_address());
                        }
                        Event::Tick => {
                            if let Some((pawn_key, _)) = self.pawn {
                                if let Some(command) = self.queued_command.take() {
                                    self.client.send_command(&pawn_key, &command);
                                }
                            }
                        }
                        Event::AssignPawn(local_key) => {
                            info!("assign pawn");
                            if let Some(typed_object) = self.client.get_pawn_mut(&local_key) {
                                match typed_object {
                                    Protocol::Square(square_ref) => {
                                        self.pawn = Some((local_key, square_ref.clone()));
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Event::UnassignPawn(_) => {
                            self.pawn = None;
                            info!("unassign pawn");
                        }
                        Event::NewCommand(_, command_type) => match command_type {
                            Protocol::KeyCommand(key_command) => {
                                if let Some((_, pawn_ref)) = &self.pawn {
                                    shared_behavior::process_command(&key_command, &pawn_ref);
                                }
                            }
                            _ => {}
                        },
                        Event::ReplayCommand(_, command_type) => match command_type {
                            Protocol::KeyCommand(key_command) => {
                                if let Some((_, pawn_ref)) = &self.pawn {
                                    shared_behavior::process_command(&key_command, &pawn_ref);
                                }
                            }
                            _ => {}
                        },
                        Event::CreateObject(local_key) => {
                            if let Some(typed_object) = self.client.get_object(&local_key) {
                                match typed_object {
                                    Protocol::Square(square_ref) => {
                                        self.square_map.insert(local_key, square_ref.clone());
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Event::DeleteObject(local_key, _) => {
                            self.square_map.remove(&local_key);
                        }
                        _ => {}
                    },
                    Err(err) => {
                        info!("Client Error: {}", err);
                    }
                }
            } else {
                break;
            }
        }

        // drawing
        clear_background(BLACK);

        let square_size = 32.0;

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
                    square_size,
                    square_size,
                    color,
                );
            }

            // draw pawn
            if let Some((_, pawn_ref)) = &self.pawn {
                let square = pawn_ref.borrow();
                draw_rectangle(
                    f32::from(*(square.x.get())),
                    f32::from(*(square.y.get())),
                    square_size,
                    square_size,
                    WHITE,
                );
            }
        }
    }
}
