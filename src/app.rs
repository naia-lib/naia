
use log::{info};

use gaia_client::GaiaClient;

pub struct App {
    client: GaiaClient,
    count: u8,
}

impl App {
    pub fn new() -> App {
        let mut app = App {
            client: GaiaClient::new(),
            count: 0,
        };

        info!("App Start");

        app
    }

    pub fn update(&mut self) {
        if self.count > 10 {
            return;
        }
        self.count += 1;
        info!("App Update {}", self.count);
    }
}