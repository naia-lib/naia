use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use log::info;

use naia_client::{ClientConfig, ClientEvent, Client};

use naia_example_shared::{
    get_shared_config, manifest_load,
    components::{Components, Position, Name},
    events::{Events, Auth, StringMessage},
};

const SERVER_PORT: u16 = 14191;

pub struct App {
    client: Client<Events, Components>,
    server_event_count: u32,
}

impl App {
    pub fn new() -> App {
        info!("Naia Client Example Started");

        // Put your Server's IP Address here!, can't easily find this automatically from
        // the browser
        let server_ip_address: IpAddr = "127.0.0.1"
            .parse()
            .expect("couldn't parse input IP address");
        let server_socket_address = SocketAddr::new(server_ip_address, SERVER_PORT);

        let mut client_config = ClientConfig::default();
        client_config.heartbeat_interval = Duration::from_secs(2);
        // Keep in mind that the disconnect timeout duration should always be at least
        // 2x greater than the heartbeat interval, to make it so at the worst case, the
        // server would need to miss 2 heartbeat signals before disconnecting from a
        // given client
        client_config.disconnection_timeout_duration = Duration::from_secs(5);

        // This will be evaluated in the Server's 'on_auth()' method
        let auth = Events::Auth(Auth::new("charlie", "12345"));

        App {
            client: Client::new(
                server_socket_address,
                manifest_load(),
                Some(client_config),
                get_shared_config(),
                Some(auth),
            ),
            server_event_count: 0,
        }
    }

    // Currently, this will call every frame. On Linux it's called in a loop. On Web
    // it's called via request_animation_frame()
    pub fn update(&mut self) {
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
                        ClientEvent::Event(event_type) => match event_type {
                            Events::StringMessage(string_event) => {
                                let _message = string_event.message.get();
                                //info!("Client received event: {}", message);

                                let new_message =
                                    format!("Client Packet ({})", self.server_event_count);
                                //info!("Client send: {}", new_message);

                                let string_event = StringMessage::new(new_message);
                                self.client.send_event(&string_event);
                                self.server_event_count += 1;
                            }
                            _ => {}
                        },
                        ClientEvent::CreateEntity(entity_key) => {
                            info!("creation of entity: {}", entity_key);
                        },
                        ClientEvent::DeleteEntity(entity_key) => {
                            info!("deletion of entity: {}", entity_key);
                        },
                        ClientEvent::AddComponent(entity_key, component_key) => {
                            info!("add component: {}, to entity: {}", component_key, entity_key);
                        },
                        ClientEvent::UpdateComponent(entity_key, component_key) => {
                            info!("update component: {}, to entity: {}", component_key, entity_key);
                        },
                        ClientEvent::RemoveComponent(entity_key, component_key) => {
                            info!("remove component: {}, from entity: {}", component_key, entity_key);
                        },
                        ClientEvent::CreateActor(entity_key) => {
                            info!("creation of actor: {}", entity_key);
                        },
                        ClientEvent::DeleteActor(entity_key) => {
                            info!("deletion of actor: {}", entity_key);
                        },
                        ClientEvent::UpdateActor(entity_key) => {
                            info!("update of actor: {}", entity_key);
                        },

//                        ClientEvent::UpdateActor(local_key) => {
//                            if let Some(actor) = self.client.get_actor(&local_key) {
//                                match actor {
//                                    Components::Position(point_actor) => {
//                                        info!("update of point actor with key: {}, x:{}, y: {}, name: {} {}",
//                                              local_key,
//                                              point_actor.borrow().x.get(),
//                                              point_actor.borrow().y.get(),
//                                              point_actor.borrow().name.get().first,
//                                              point_actor.borrow().name.get().last);
//                                    }
//                                }
//                            }
//                        }
                        ClientEvent::Tick => {
                            //info!("tick event");
                        }
                        _ => {}
                    },
                    Err(err) => {
                        info!("Client Error: {}", err);
                        return;
                    }
                }
            } else {
                break;
            }
        }
    }
}
