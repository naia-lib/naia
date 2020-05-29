
use log::{info};

use gaia_client::{GaiaClient, ClientEvent, Packet, Config};

use std::time::Duration;

pub struct App {
    client: GaiaClient,
}

impl App {
    pub fn new(server_socket_address: &str) -> App {

        info!("App Start");

        let mut config = Config::default();
        config.heartbeat_interval = Duration::from_secs(2);

        App {
            client: GaiaClient::connect(&server_socket_address, Some(config)),
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

                        let count = self.client.get_sequence_number();
                        let to_server_message: String = "Client Packet (".to_string() + count.to_string().as_str() + ")";
                        info!("Client send: {}", to_server_message);
                        self.client.send(Packet::new(to_server_message.into_bytes()));
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