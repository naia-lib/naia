
use log::{info};

use std::time::Duration;

use gaia_client::{GaiaClient, ClientEvent, Packet, Config};

use gaia_example_shared::{manifest_load, StringEvent, ExampleType};

pub struct App {
    client: GaiaClient<ExampleType>,
}

impl App {
    pub fn new(server_socket_address: &str) -> App {

        info!("App Start");

        let mut config = Config::default();
        config.heartbeat_interval = Duration::from_secs(2);

        App {
            client: GaiaClient::connect(&server_socket_address, manifest_load(), Some(config)),
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
                            ExampleType::StringEvent(string_event) => {
                                let message = string_event.get_message();
                                match message {
                                    Some(msg) => {
                                        info!("Client received event: {}", msg);

                                        if let Some(count) = self.client.get_sequence_number() {
                                            let new_message: String = "Client Packet (".to_string() + count.to_string().as_str() + ")";
                                            info!("Client send: {}", new_message);

                                            let string_event = StringEvent::new(new_message);
                                            self.client.send_event(&string_event);

                                            //self.client.send(Packet::new(to_server_message.into_bytes()));
                                        }
                                    }
                                    None => {}
                                }
                            }
                        }
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