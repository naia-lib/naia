use log::info;

use hecs::{Entity, World};

use naia_hecs_client::{Client as NaiaClient, ClientConfig, WorldData, shared::DefaultChannels};

use naia_hecs_demo_shared::{
    protocol::{Auth, Protocol},
    shared_config,
};

use super::systems::{events::process_events, startup::app_init};

pub type Client = NaiaClient<Protocol, Entity, DefaultChannels>;

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
            shared_config(),
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
