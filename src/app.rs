
use log::{info};

use gaia_client::GaiaClient;

const PING_MSG: &str = "ping";
const PONG_MSG: &str = "pong";

pub struct App {
    client: GaiaClient,
}

impl App {
    pub fn new() -> App {
        let mut app = App {
            client: GaiaClient::new(),
        };

        app
    }

    pub fn update(&mut self) {
        //
    }
}