use bevy::{ecs::schedule::ShouldRun, log::info};

use crate::aliases::Server;

pub fn should_tick(server: Server) -> ShouldRun {
    if server.has_ticked() {
        return ShouldRun::Yes;
    } else {
        return ShouldRun::No;
    }
}

pub fn tick(mut _server: Server) {
    // All game logic should happen here, on a tick event
    info!("tick");
}
