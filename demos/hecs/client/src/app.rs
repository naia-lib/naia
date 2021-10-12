use log::info;

use hecs::World;

use naia_hecs_client::{Entity, Client as NaiaClient, ClientConfig, WorldData};

use naia_hecs_demo_shared::{
    get_server_address, get_shared_config,
    protocol::{Auth, Protocol},
};

use super::systems::{
    startup::app_init,
    events::process_events,
};

pub type Client = NaiaClient<Protocol, Entity>;

pub struct App {
    pub client: Client,
    pub world: World,
    pub message_count: u32,
    pub world_data: WorldData,
}

impl App {
    pub fn new() -> Self {
        info!("Naia Hecs Client Demo started");

        app_init(
            ClientConfig::default(),
            get_shared_config(),
            get_server_address(),
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
