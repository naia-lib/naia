use bevy_ecs::{event::EventReader, system::Query};
use bevy_log::info;

use naia_bevy_server::{events::TickEvent, Server};

use naia_bevy_demo_shared::{
    behavior as shared_behavior, channels::PlayerCommandChannel, components::Position,
    messages::KeyCommand,
};

pub fn tick_events(
    mut server: Server,
    mut position_query: Query<&mut Position>,
    mut tick_reader: EventReader<TickEvent>,
) {
    let mut has_ticked = false;

    for TickEvent(server_tick) in tick_reader.iter() {
        has_ticked = true;

        let mut processed = false;

        let messages = server.receive_tick_buffer_messages(server_tick);
        for (_user_key, key_command) in messages.read::<PlayerCommandChannel, KeyCommand>() {
            if let Some(entity) = &key_command.entity.get(&server) {
                if let Ok(mut position) = position_query.get_mut(*entity) {
                    shared_behavior::process_command(&key_command, &mut position);
                    processed = true;
                }
            }
        }

        if !processed {
            info!("Tick without Command: {server_tick}");
        }
    }

    if has_ticked {
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
}
