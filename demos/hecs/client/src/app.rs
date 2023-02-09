use std::collections::HashMap;

use log::info;

use hecs::Entity;

use naia_hecs_client::{
    Client as NaiaClient, ClientConfig, WorldWrapper as World,
};

use naia_hecs_demo_shared::{
    protocol, Auth,
};

use super::systems::{events::process_events, startup::app_init};

pub type Client = NaiaClient<Entity>;

pub struct App {
    pub client: Client,
    pub world: World,
    pub message_count: u32,
    pub entity_to_id_map: HashMap<Entity, u32>,
    pub next_id: u32,
}

impl App {
    pub fn default() -> Self {
        info!("Naia Hecs Client Demo started");

        app_init(
            ClientConfig::default(),
            protocol(),
            "http://127.0.0.1:14191",
            Auth::new("charlie", "12345"),
        )
    }

    pub fn update(&mut self) {
        process_events(self);
    }

    pub fn tick(&mut self) {
        //info!("tick");
    }
}
