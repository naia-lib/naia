
use log::{info};

use std::time::Duration;

use gaia_client::{GaiaClient, ClientEvent, Packet, Config};

use gaia_example_shared::{manifest_load, ExampleType};

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
                    ClientEvent::Message(message) => {
                        info!("Client recv: {}", message);

                        if let Some(count) = self.client.get_sequence_number() {
                            let to_server_message: String = "Client Packet (".to_string() + count.to_string().as_str() + ")";
                            info!("Client send: {}", to_server_message);
                            self.client.send(Packet::new(to_server_message.into_bytes()));
                        }
                    }
                    ClientEvent::Event(incoming_type) => {
                        match incoming_type {
                            ExampleType::StringEvent(incoming) => {
                                let message = incoming.get_message();
                                match message {
                                    Some(msg) => {
                                        info!("CLIENT RECEIVED EVENT: {}", msg);
                                    }
                                    None => {}
                                }
                            }
                        }
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