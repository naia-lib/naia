
use log::{info};

use std::time::Duration;
use std::ops::Deref;
use std::cell::RefCell;
use std::rc::Rc;

use gaia_client::{GaiaClient, ClientEvent, Config};

use gaia_example_shared::{event_manifest_load, entity_manifest_load, StringEvent, ExampleEvent, ExampleEntity};

pub struct App {
    client: GaiaClient<ExampleEvent, ExampleEntity>,
}

impl App {
    pub fn new(server_socket_address: &str) -> App {

        info!("App Start");

        let mut config = Config::default();
        config.heartbeat_interval = Duration::from_secs(4);

        App {
            client: GaiaClient::connect(&server_socket_address, event_manifest_load(), entity_manifest_load(), Some(config)),
        }
    }

    pub fn update(&mut self) {

        match self.client.receive() {
            Ok(event) => {
                match event {
                    ClientEvent::Connection => {
                        info!("Client connected to: {}", self.client.server_address());
                    }
                    ClientEvent::Disconnection => {
                        info!("Client disconnected from: {}", self.client.server_address());
                    }
                    ClientEvent::Event(event_type) => {
                        match event_type {
                            ExampleEvent::StringEvent(string_event) => {
                                let message = string_event.get_message();
                                match message {
                                    Some(msg) => {
                                        info!("Client received event: {}", msg);

                                        if let Some(count) = self.client.get_sequence_number() {
                                            let new_message: String = "Client Packet (".to_string() + count.to_string().as_str() + ")";
                                            info!("Client send: {}", new_message);

                                            let string_event = StringEvent::new(new_message);
                                            self.client.send_event(&string_event);
                                        }
                                    }
                                    None => {}
                                }
                            }
                        }
                    }
                    ClientEvent::CreateEntity(local_key, entity) => {
                        match entity {
                            // keep in mind that the below values are clones of the original, used purely to determine the type
                            ExampleEntity::PointEntity(point_entity) => {
                                info!("creation of point entity with x: {}, y: {}",
                                      point_entity.as_ref().borrow().get_x(),
                                      point_entity.as_ref().borrow().get_y());
                            }
                            _ => {}
                        }
                    }
                    ClientEvent::UpdateEntity(local_key) => {
                        if let Some(entity) = self.client.get_entity(local_key) {
                            match entity {
                                // keep in mind that the below values are clones of the original, used purely to determine the type
                                ExampleEntity::PointEntity(point_entity) => {
                                    info!("update of point entity with x: {}, y: {}",
                                          point_entity.as_ref().borrow().get_x(),
                                          point_entity.as_ref().borrow().get_y());
                                }
                                _ => {}
                            }
                        }
                    }
                    ClientEvent::DeleteEntity(local_key) => {
                        info!("deletion of point entity");
                    }
                    ClientEvent::None => {
                        //info!("Client non-event");
                    }
                    _ => { }
                }
            }
            Err(err) => {
                info!("Client Error: {}", err);
            }
        }
    }
}