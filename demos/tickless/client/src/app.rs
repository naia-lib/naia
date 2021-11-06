use std::collections::HashSet;

use naia_client::{Client as NaiaClient, ClientConfig, Event};

use naia_default_world::{Entity, World as DefaultWorld, WorldMutType, WorldRefType};

use naia_macroquad_demo_shared::{
    get_server_address, get_shared_config,
    protocol::{Protocol, Text},
};

type World = DefaultWorld<Protocol>;
type Client = NaiaClient<Protocol, Entity>;

pub struct App {
    client: Client,
    world: World,
}

impl App {
    pub fn new() -> Self {
        info!("Naia Tickless Client Demo started");

        let server_address = get_server_address();

        let mut client = Client::new(ClientConfig::default(), get_shared_config());
        client.connect(server_address, None);

        App {
            client,
            world: World::new(),
        }
    }

    pub fn update(&mut self) {
        for event in self.client.receive(&mut self.world.proxy_mut()) {
            match event {
                Ok(Event::Connection) => {
                    info!("Client connected to: {}", self.client.server_address());
                }
                Ok(Event::Disconnection) => {
                    info!("Client disconnected from: {}", self.client.server_address());
                }
                Ok(Event::Tick) => {
                    info!("TICK SHOULD NOT HAPPEN!");
                }
                Ok(Event::Message(_, Protocol::Text(text))) => {
                    info!("message: {}", text.value.get());
                }
                Err(err) => {
                    info!("Client Error: {}", err);
                }
                _ => {}
            }
        }
    }
}
