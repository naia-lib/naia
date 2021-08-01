use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
    collections::HashMap,
};

use macroquad::prelude::*;

use naia_client::{ClientConfig, ClientEvent, NaiaClient, Ref};

use naia_mq_example_shared::{
    get_shared_config, manifest_load, shared_behavior, AuthEvent, ExampleActor, ExampleEvent,
    KeyCommand, PointActorColor, PointActor
};

const SERVER_PORT: u16 = 14193;

pub struct App {
    client: NaiaClient<ExampleEvent, ExampleActor>,
    pawn: Option<(u16, Ref<PointActor>)>,
    queued_command: Option<KeyCommand>,
    actors: HashMap<u16, Ref<PointActor>>,
}

impl App {
    pub fn new() -> Self {
        info!("Naia Macroquad Client Example Started");

        // Put your Server's IP Address here!, can't easily find this automatically from the browser
        let server_ip_address: IpAddr = "127.0.0.1"
            .parse()
            .expect("couldn't parse input IP address");
        let server_socket_address = SocketAddr::new(server_ip_address, SERVER_PORT);

        let mut client_config = ClientConfig::default();
        client_config.heartbeat_interval = Duration::from_secs(2);
        client_config.disconnection_timeout_duration = Duration::from_secs(5);

        let auth = ExampleEvent::AuthEvent(AuthEvent::new("charlie", "12345"));

        let client = NaiaClient::new(
            server_socket_address,
            manifest_load(),
            Some(client_config),
            get_shared_config(),
            Some(auth),
        );

        App {
            client,
            pawn: None,
            queued_command: None,
            actors: HashMap::new(),
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
                                    self.client.send_command(pawn_key, &command);
                                }
                            }
                        }
                        ClientEvent::AssignPawn(local_key) => {
                            info!("assign pawn");
                            if let Some(typed_actor) = self.client.get_pawn_mut(&local_key) {
                                match typed_actor {
                                    ExampleActor::PointActor(actor_ref) => {
                                        self.pawn = Some((local_key, actor_ref.clone()));
                                    }
                                }
                            }
                        }
                        ClientEvent::UnassignPawn(_) => {
                            self.pawn = None;
                            info!("unassign pawn");
                        }
                        ClientEvent::NewCommand(_, command_type) => match command_type {
                            ExampleEvent::KeyCommand(key_command) => {
                                if let Some((_, pawn_ref)) = &self.pawn {
                                    shared_behavior::process_command(&key_command, &pawn_ref);
                                }
                            }
                            _ => {}
                        },
                        ClientEvent::ReplayCommand(_, command_type) => match command_type {
                            ExampleEvent::KeyCommand(key_command) => {
                                if let Some((_, pawn_ref)) = &self.pawn {
                                    shared_behavior::process_command(&key_command, &pawn_ref);
                                }
                            }
                            _ => {}
                        },
                        ClientEvent::CreateActor(local_key) => {
                            if let Some(typed_actor) = self.client.get_actor(&local_key) {
                                match typed_actor {
                                    ExampleActor::PointActor(actor_ref) => {
                                        self.actors.insert(local_key, actor_ref.clone());
                                    }
                                }
                            }
                        },
                        ClientEvent::DeleteActor(local_key) => {
                            self.actors.remove(&local_key);
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
            // draw actors
            for (_, actor_ref) in &self.actors {
                let point_actor = actor_ref.borrow();
                let color = match point_actor.color.get() {
                    PointActorColor::Red => RED,
                    PointActorColor::Blue => BLUE,
                    PointActorColor::Yellow => YELLOW,
                };
                draw_rectangle(
                    f32::from(*(point_actor.x.get())),
                    f32::from(*(point_actor.y.get())),
                    square_size,
                    square_size,
                    color,
                );
            }

            // draw pawns
            if let Some((_, pawn_ref)) = &self.pawn {
                let point_actor = pawn_ref.borrow();
                draw_rectangle(
                    f32::from(*(point_actor.x.get())),
                    f32::from(*(point_actor.y.get())),
                    square_size,
                    square_size,
                    WHITE,
                );
            }
        }
    }
}
