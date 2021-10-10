use bevy::{ecs::schedule::ShouldRun, log::info};

use naia_bevy_client::Client;

use naia_bevy_demo_shared::protocol::Protocol;

pub fn should_tick(client: Client<Protocol>) -> ShouldRun {
    if client.has_ticked() {
        return ShouldRun::Yes;
    } else {
        return ShouldRun::No;
    }
}

pub fn tick(mut _client: Client<Protocol>) {
    // All game logic should happen here, on a tick event
    info!("tick");
}
