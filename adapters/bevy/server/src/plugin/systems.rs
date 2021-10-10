use bevy::ecs::schedule::ShouldRun;

use naia_server::ProtocolType;

use crate::Server;

pub fn should_tick<P: ProtocolType>(server: Server<P>) -> ShouldRun {
    if server.has_ticked() {
        return ShouldRun::Yes;
    } else {
        return ShouldRun::No;
    }
}
