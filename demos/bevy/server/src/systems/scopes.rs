use bevy::prelude::*;

use crate::aliases::Server;

pub fn update_scopes(mut server: ResMut<Server>) {
    // Update scopes of entities
    for (_, user_key, entity_key) in server.scope_checks() {
        // You'd normally do whatever checks you need to in here..
        // to determine whether each Entity should be in scope or not.

        // This indicates the Entity should be in this scope.
        server.user_scope(&user_key).include(&entity_key);

        // And call this if Entity should NOT be in this scope.
        // server.user_scope(..).exclude(..);
    }
}
