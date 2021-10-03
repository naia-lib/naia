use std::collections::HashSet;

use hecs::World;

use naia_server::{RoomKey, Server as NaiaServer};

use naia_hecs_demo_shared::protocol::Protocol;

use naia_hecs_server::Entity;

use super::systems::{
    events::process_events,
    scopes::update_scopes,
    startup::app_init,
    tick::{march_and_mark, send_messages, send_updates},
};

pub type Server = NaiaServer<Protocol, Entity>;

pub struct App {
    pub server: Server,
    pub world: World,
    pub main_room_key: RoomKey,
    pub tick_count: u32,
    pub has_marker: HashSet<Entity>,
}

impl App {
    pub fn new() -> Self {
        info!("Naia Hecs Server Demo started");
        app_init()
    }

    pub fn update(&mut self) {
        process_events(self);
    }

    pub fn tick(&mut self) {
        march_and_mark(self);
        send_messages(self);
        update_scopes(self);
        send_updates(self);
    }
}
