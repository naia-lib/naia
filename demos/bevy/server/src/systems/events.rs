use bevy_ecs::{
    event::EventReader,
    system::{Query, ResMut},
};
use bevy_log::info;

use naia_bevy_server::{
    events::{
        AuthEvents, ConnectEvent, DisconnectEvent, ErrorEvent, InsertComponentEvents,
        SpawnEntityEvent, TickEvent, UpdateComponentEvents,
    },
    Random, Server,
};

use naia_bevy_demo_shared::{
    behavior as shared_behavior,
    channels::{EntityAssignmentChannel, PlayerCommandChannel},
    components::{Color, ColorValue, Position},
    messages::{Auth, EntityAssignment, KeyCommand},
};

use crate::resources::Global;

pub fn auth_events(mut event_reader: EventReader<AuthEvents>, mut server: Server) {
    for events in event_reader.iter() {
        for (user_key, auth) in events.read::<Auth>() {
            if auth.username == "charlie" && auth.password == "12345" {
                // Accept incoming connection
                server.accept_connection(&user_key);
            } else {
                // Reject incoming connection
                server.reject_connection(&user_key);
            }
        }
    }
}

pub fn connect_events(
    mut event_reader: EventReader<ConnectEvent>,
    mut global: ResMut<Global>,
    mut server: Server,
) {
    for ConnectEvent(user_key) in event_reader.iter() {
        let address = server
            .user_mut(user_key)
            // Add User to the main Room
            .enter_room(&global.main_room_key)
            // Get User's address for logging
            .address();

        info!("Naia Server connected to: {}", address);

        // Create components for Entity to represent new player

        // Position component
        let position = {
            let x = 16 * ((Random::gen_range_u32(0, 40) as i16) - 20);
            let y = 16 * ((Random::gen_range_u32(0, 30) as i16) - 15);
            Position::new(x, y)
        };

        // Color component
        let color = {
            let color_value = match server.users_count() % 3 {
                0 => ColorValue::Yellow,
                1 => ColorValue::Red,
                _ => ColorValue::Blue,
            };
            Color::new(color_value)
        };

        // Spawn entity
        let entity = server
            // Spawn new Square Entity
            .spawn()
            // Add Entity to main Room
            .enter_room(&global.main_room_key)
            // Insert Position component
            .insert(position)
            // Insert Color component
            .insert(color)
            // return Entity id
            .id();

        global.user_to_entity_map.insert(*user_key, entity);

        // Send an Entity Assignment message to the User that owns the Square
        let mut assignment_message = EntityAssignment::new(true);
        assignment_message.entity.set(&server, &entity);

        server.send_message::<EntityAssignmentChannel, EntityAssignment>(
            user_key,
            &assignment_message,
        );
    }
}

pub fn disconnect_events(
    mut event_reader: EventReader<DisconnectEvent>,
    mut global: ResMut<Global>,
    mut server: Server,
) {
    for DisconnectEvent(user_key, user) in event_reader.iter() {
        info!("Naia Server disconnected from: {:?}", user.address);

        if let Some(entity) = global.user_to_entity_map.remove(user_key) {
            server
                .entity_mut(&entity)
                .leave_room(&global.main_room_key)
                .despawn();
        }
    }
}

pub fn error_events(mut event_reader: EventReader<ErrorEvent>) {
    for ErrorEvent(error) in event_reader.iter() {
        info!("Naia Server Error: {:?}", error);
    }
}

pub fn tick_events(
    mut server: Server,
    mut position_query: Query<&mut Position>,
    mut tick_reader: EventReader<TickEvent>,
) {
    let mut has_ticked = false;

    for TickEvent(server_tick) in tick_reader.iter() {
        has_ticked = true;

        // All game logic should happen here, on a tick event

        let messages = server.receive_tick_buffer_messages(server_tick);
        for (_user_key, key_command) in messages.read::<PlayerCommandChannel, KeyCommand>() {
            let Some(entity) = &key_command.entity.get(&server) else {
                continue;
            };
            let Ok(mut position) = position_query.get_mut(*entity) else {
                continue;
            };
            shared_behavior::process_command(&key_command, &mut position);
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

pub fn spawn_entity_events(mut event_reader: EventReader<SpawnEntityEvent>) {
    for SpawnEntityEvent(_) in event_reader.iter() {
        info!("spawned client entity");
    }
}

pub fn insert_component_events(
    mut event_reader: EventReader<InsertComponentEvents>,
    mut global: ResMut<Global>,
    mut server: Server,
    position_query: Query<&Position>,
) {
    for events in event_reader.iter() {
        for client_entity in events.read::<Position>() {
            info!("insert component into client entity");

            if let Ok(client_position) = position_query.get(client_entity) {
                // New Position Component
                let server_position = Position::new(*client_position.x, *client_position.y);

                // New Color component
                let color = {
                    let color_value = match server.users_count() % 3 {
                        0 => ColorValue::Yellow,
                        1 => ColorValue::Red,
                        _ => ColorValue::Blue,
                    };
                    Color::new(color_value)
                };

                // Spawn entity
                let server_entity = server
                    // Spawn new Square Entity
                    .spawn()
                    // Add Entity to main Room
                    .enter_room(&global.main_room_key)
                    // Insert Position component
                    .insert(server_position)
                    // Insert Color component
                    .insert(color)
                    // return Entity id
                    .id();

                global.echo_entity_map.insert(client_entity, server_entity);
            }
        }
    }
}

pub fn update_component_events(
    mut event_reader: EventReader<UpdateComponentEvents>,
    global: ResMut<Global>,
    mut position_query: Query<&mut Position>,
) {
    for events in event_reader.iter() {
        for (_, client_entity) in events.read::<Position>() {
            if let Some(server_entity) = global.echo_entity_map.get(&client_entity) {
                if let Ok([client_position, mut server_position]) =
                    position_query.get_many_mut([client_entity, *server_entity])
                {
                    server_position.x.mirror(&client_position.x);
                    server_position.y.mirror(&client_position.y);
                }
            }
        }
    }
}
