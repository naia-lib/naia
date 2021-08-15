extern crate cfg_if;

use std::time::Duration;

use macroquad::prelude::*;

use naia_client::ClientConfig;
use naia_demo_macroquad_shared::get_server_address;

mod app;
use app::App;

#[macroquad::main("NaiaMacroquadExample")]
async fn main() {
    info!("Naia Macroquad Client Example Started");

    let mut client_config = ClientConfig::default();

    client_config.server_address = get_server_address();

    client_config.heartbeat_interval = Duration::from_secs(2);
    client_config.disconnection_timeout_duration = Duration::from_secs(5);

    let mut app = App::new(client_config);

    loop {
        clear_background(BLACK);

        app.update();

        next_frame().await
    }
}
