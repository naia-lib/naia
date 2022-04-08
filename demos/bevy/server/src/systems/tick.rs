
use naia_bevy_server::{Server, shared::DefaultChannels};

use naia_bevy_demo_shared::protocol::Protocol;

pub fn tick(mut server: Server<Protocol, DefaultChannels>) {
    // All game logic should happen here, on a tick event
    //info!("tick");

    // Update scopes of entities
    for (_, user_key, entity) in server.scope_checks() {
        // You'd normally do whatever checks you need to in here..
        // to determine whether each Entity should be in scope or not.

        // This indicates the Entity should be in this scope.
        server.user_scope(&user_key).include(&entity);

        // And call this if Entity should NOT be in this scope.
        // server.user_scope(..).exclude(..);
    }

    // This is very important! Need to call this to actually send all update packets
    // to all connected Clients!
    server.send_all_updates();
}
