use log::info;

use std::{format, time::Duration};

use naia_client::{ClientEvent, Config, NaiaClient};

use naia_example_shared::{manifest_load, AuthEvent, ExampleEntity, ExampleEvent, StringEvent};

const SERVER_PORT: &str = "14191";

#[cfg(not(target_arch = "wasm32"))]
use naia_client::find_my_ip_address;

pub struct App {
    client: NaiaClient<ExampleEvent, ExampleEntity>,
    server_event_count: u32,
}

impl App {
    pub fn new() -> App {
        info!("Naia Client Example Started");

        cfg_if! {
            if #[cfg(target_arch = "wasm32")] {
                let server_ip_address: &str = "192.168.1.9"; // Put your Server's IP Address here!, can't easily find this automatically from the browser
            } else {
                let current_ip_address_string = find_my_ip_address().expect("can't find ip address").to_string();
                let server_ip_address: &str = current_ip_address_string.as_str();
            }
        }

        let server_socket_address = format!("{}:{}", server_ip_address, SERVER_PORT);

        let mut config = Config::default();
        config.heartbeat_interval = Duration::from_secs(4);

        let auth = ExampleEvent::AuthEvent(AuthEvent::new("charlie", "12345"));

        App {
            client: NaiaClient::new(
                &server_socket_address,
                manifest_load(),
                Some(config),
                Some(auth),
            ),
            server_event_count: 0,
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
                    ClientEvent::Event(event_type) => match event_type {
                        ExampleEvent::StringEvent(string_event) => {
                            let message = string_event.message.get();
                            info!("Client received event: {}", message);

                            let new_message: String = "Client Packet (".to_string()
                                + self.server_event_count.to_string().as_str()
                                + ")";
                            info!("Client send: {}", new_message);

                            let string_event = StringEvent::new(new_message);
                            self.client.send_event(&string_event);
                            self.server_event_count += 1;
                        }
                        _ => {}
                    },
                    ClientEvent::CreateEntity(local_key) => {
                        if let Some(entity) = self.client.get_entity(local_key) {
                            match entity {
                                ExampleEntity::PointEntity(point_entity) => {
                                    info!(
                                        "creation of point entity with key: {}, x: {}, y: {}",
                                        local_key,
                                        point_entity.as_ref().borrow().x.get(),
                                        point_entity.as_ref().borrow().y.get()
                                    );
                                }
                            }
                        }
                    }
                    ClientEvent::UpdateEntity(local_key) => {
                        if let Some(entity) = self.client.get_entity(local_key) {
                            match entity {
                                ExampleEntity::PointEntity(point_entity) => {
                                    info!(
                                        "update of point entity with key: {}, x: {}, y: {}",
                                        local_key,
                                        point_entity.as_ref().borrow().x.get(),
                                        point_entity.as_ref().borrow().y.get()
                                    );
                                }
                            }
                        }
                    }
                    ClientEvent::DeleteEntity(local_key) => {
                        info!("deletion of point entity with key: {}", local_key);
                    }
                    ClientEvent::None => {
                        //info!("Client non-event");
                    }
                }
            }
            Err(err) => {
                info!("Client Error: {}", err);
            }
        }
    }
}
