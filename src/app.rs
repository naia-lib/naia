
use log::{info};

use gaia_client::{GaiaClient, ClientEvent};

const PING_MSG: &str = "ping";
const PONG_MSG: &str = "pong";

pub struct App {
    client: GaiaClient,
    count: u8,
}

impl App {
    pub fn new(server_socket_address: &str) -> App {

        info!("App Start");

        App {
            client: GaiaClient::connect(&server_socket_address),
            count: 0,
        }
    }

    pub fn update(&mut self) {

        match self.client.receive() {
            Ok(event) => {
                match event {
                    ClientEvent::Connection => {
                        info!("Client connected to: {}", self.client.server_address());
                        self.count += 1;
                        let to_server_message: String = "Client Packet ".to_string() + self.count.to_string().as_str();
                        info!("Client send: {}", to_server_message);
                        self.client.send(to_server_message);
                    }
                    ClientEvent::Disconnection => {
                        info!("Client disconnected from: {}", self.client.server_address());
                    }
                    ClientEvent::Message(message) => {
                        info!("Client recv: {}", message);

                        if self.count < 10 {
                            self.count += 1;
                            let to_server_message: String = "Client Packet ".to_string() + self.count.to_string().as_str();
                            info!("Client send: {}", to_server_message);
                            self.client.send(to_server_message);
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