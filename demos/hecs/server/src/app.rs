use std::collections::HashSet;

use hecs::Entity;

use naia_hecs_server::{
    RoomKey, Server as NaiaServer, ServerAddrs, ServerConfig, WorldWrapper as World,
};

use naia_hecs_demo_shared::protocol;

use super::systems::{
    events::process_events,
    startup::app_init,
    tick::{check_scopes, march_and_mark, send_updates},
};

pub type Server = NaiaServer<Entity>;

pub struct App {
    pub has_user: bool,
    pub server: Server,
    pub world: World,
    pub main_room_key: RoomKey,
    pub tick_count: u32,
    pub has_marker: HashSet<Entity>,
}

impl App {
    pub fn new() -> Self {
        info!("Naia Hecs Server Demo started");

        let server_addresses = ServerAddrs::new(
            "127.0.0.1:14191"
                .parse()
                .expect("could not parse Signaling address/port"),
            // IP Address to listen on for UDP WebRTC data channels
            "127.0.0.1:14192"
                .parse()
                .expect("could not parse WebRTC data address/port"),
            // The public WebRTC IP address to advertise
            "http://127.0.0.1:14192",
        );

        app_init(ServerConfig::default(), protocol(), server_addresses)
    }

    pub fn update(&mut self) {
        process_events(self);
    }

    pub fn tick(&mut self) {
        march_and_mark(self);
        check_scopes(self);
        send_updates(self);
    }
}
