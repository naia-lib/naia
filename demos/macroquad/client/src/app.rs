use std::collections::HashMap;

use macroquad::prelude::*;

use naia_client::{Replicate, ClientConfig, Event, Client, Ref, LocalReplicateKey};

use naia_demo_macroquad_shared::{
    get_shared_config, behavior as shared_behavior, protocol::{Protocol, Square, Color, Auth, KeyCommand},
};

pub struct App {
    client: Client<Protocol>,
    pawn: Option<(LocalReplicateKey, Ref<Square>)>,
    queued_command: Option<KeyCommand>,
    square_map: HashMap<LocalReplicateKey, Ref<Square>>,
}

impl App {
    pub fn new(client_config: ClientConfig) -> Self {

        let auth = Auth::new("charlie", "12345").get_typed_copy();

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
                                        self.replicates.insert(local_key, square_ref.clone());
                                    }
                                    _ => {}
                                }
                            }
                        },
                        Event::DeleteObject(local_key, _) => {
                            self.replicates.remove(&local_key);
                        },
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
            // draw replicates
            for (_, square_ref) in &self.replicates {
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

            // draw pawns
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
