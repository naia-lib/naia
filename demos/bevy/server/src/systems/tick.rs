use bevy_ecs::{event::EventReader, system::{Query, Local}};

use naia_bevy_server::{Server, events::TickEvent, Tick};
use naia_bevy_demo_shared::{behavior as shared_behavior, components::Position, channels::PlayerCommandChannel, messages::KeyCommand};

use crate::info;

pub fn tick_events(
    mut server: Server,
    mut tick_reader: EventReader<TickEvent>,
    mut position_query: Query<&mut Position>,
    mut last_server_tick: Local<Tick>
) {
    let mut has_ticked = false;

    for TickEvent(server_tick) in tick_reader.iter() {

        //
        let local_last_server_tick: Tick = *last_server_tick;
        if *server_tick != local_last_server_tick.wrapping_add(1) {
            info!("Skipped? Last Tick at: {local_last_server_tick}, Current Tick at {server_tick}");
        }
        *last_server_tick = *server_tick;
        //

        has_ticked = true;

        // Process all received commands
        let mut processed = false;

        let messages = server.tick_buffer_messages(server_tick);
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
