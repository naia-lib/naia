use bevy::log::info;

use naia_bevy_demo_shared::protocol::Protocol;
use naia_bevy_server::Server;

pub fn tick(mut _server: Server<Protocol>) {
    // All game logic should happen here, on a tick event
    info!("tick");
}
