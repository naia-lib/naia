use std::collections::HashMap;

use macroquad::prelude::*;

use naia_client::{ClientConfig, ClientEvent, Client, Ref, LocalObjectKey};

use naia_demo_macroquad_shared::{
    get_shared_config, manifest_load, behavior as shared_behavior, events as shared_events, objects as shared_objects,
};
use shared_events::{Events, KeyCommand, Auth};
use shared_objects::{Objects, Point, Color};

pub struct App {
    client: Client<Events, Objects>,
    pawn: Option<(LocalObjectKey, Ref<Point>)>,
    queued_command: Option<KeyCommand>,
    states: HashMap<LocalObjectKey, Ref<Point>>,
}

impl App {
    pub fn new(client_config: ClientConfig) -> Self {

        let auth = Events::Auth(Auth::new("charlie", "12345"));

        let client = Client::new(
            manifest_load(),
            Some(client_config),
            get_shared_config(),
            Some(auth),
        );

        App {
            client,
            pawn: None,
            queued_command: None,
            states: HashMap::new(),
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
                        ClientEvent::Connection => {
                            info!("Client connected to: {}", self.client.server_address());
                        }
                        ClientEvent::Disconnection => {
                            info!("Client disconnected from: {}", self.client.server_address());
                        }
                        ClientEvent::Tick => {
                            if let Some((pawn_key, _)) = self.pawn {
                                if let Some(command) = self.queued_command.take() {
                                    self.client.send_command(&pawn_key, &command);
                                }
                            }
                        }
                        ClientEvent::AssignPawn(local_key) => {
                            info!("assign pawn");
                            if let Some(typed_state) = self.client.get_pawn_mut(&local_key) {
                                match typed_state {
                                    Objects::Point(state_ref) => {
                                        self.pawn = Some((local_key, state_ref.clone()));
                                    }
                                }
                            }
                        }
                        ClientEvent::UnassignPawn(_) => {
                            self.pawn = None;
                            info!("unassign pawn");
                        }
                        ClientEvent::NewCommand(_, command_type) => match command_type {
                            Events::KeyCommand(key_command) => {
                                if let Some((_, pawn_ref)) = &self.pawn {
                                    shared_behavior::process_command(&key_command, &pawn_ref);
                                }
                            }
                            _ => {}
                        },
                        ClientEvent::ReplayCommand(_, command_type) => match command_type {
                            Events::KeyCommand(key_command) => {
                                if let Some((_, pawn_ref)) = &self.pawn {
                                    shared_behavior::process_command(&key_command, &pawn_ref);
                                }
                            }
                            _ => {}
                        },
                        ClientEvent::CreateState(local_key) => {
                            if let Some(typed_state) = self.client.get_state(&local_key) {
                                match typed_state {
                                    Objects::Point(state_ref) => {
                                        self.states.insert(local_key, state_ref.clone());
                                    }
                                }
                            }
                        },
                        ClientEvent::DeleteState(local_key, _) => {
                            self.states.remove(&local_key);
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
            // draw states
            for (_, state_ref) in &self.states {
                let point_state = state_ref.borrow();
                let color = match point_state.color.get() {
                    Color::Red => RED,
                    Color::Blue => BLUE,
                    Color::Yellow => YELLOW,
                };
                draw_rectangle(
                    f32::from(*(point_state.x.get())),
                    f32::from(*(point_state.y.get())),
                    square_size,
                    square_size,
                    color,
                );
            }

            // draw pawns
            if let Some((_, pawn_ref)) = &self.pawn {
                let point_state = pawn_ref.borrow();
                draw_rectangle(
                    f32::from(*(point_state.x.get())),
                    f32::from(*(point_state.y.get())),
                    square_size,
                    square_size,
                    WHITE,
                );
            }
        }
    }
}
