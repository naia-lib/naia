use log::info;

use hecs::World;

use naia_hecs_client::{Client as NaiaClient, ClientConfig, Entity, WorldData};

use naia_hecs_demo_shared::{
    get_server_address, get_shared_config,
    protocol::{Auth, Protocol},
};

use super::systems::{events::process_events, startup::app_init};

pub type Client = NaiaClient<Protocol, Entity>;

pub struct App {
    pub client: Client,
    pub world: World,
    pub message_count: u32,
    pub world_data: WorldData<Protocol>,
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
