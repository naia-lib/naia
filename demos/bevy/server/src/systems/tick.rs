use bevy::{ecs::schedule::ShouldRun, log::info};

use naia_bevy_demo_shared::protocol::Protocol;
use naia_bevy_server::Server;

pub fn should_tick(server: Server<Protocol>) -> ShouldRun {
    if server.has_ticked() {
        return ShouldRun::Yes;
    } else {
        return ShouldRun::No;
    }
}

pub fn tick(mut _server: Server<Protocol>) {
    // All game logic should happen here, on a tick event
    info!("tick");
}
