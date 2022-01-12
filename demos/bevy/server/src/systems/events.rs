use bevy::{
    ecs::system::{Query, ResMut},
    log::info,
    prelude::*,
};

use naia_bevy_server::{
    events::{AuthorizationEvent, CommandEvent, ConnectionEvent, DisconnectionEvent},
    Random, Server,
};

use naia_bevy_demo_shared::{
    behavior as shared_behavior,
    protocol::{Color, ColorValue, Position, Protocol},
};

use crate::resources::Global;

pub fn authorization_event(
    mut event_reader: EventReader<AuthorizationEvent<Protocol>>,
    mut server: Server<Protocol>,
) {
    for event in event_reader.iter() {
        if let AuthorizationEvent(user_key, Protocol::Auth(auth_message)) = event {
            let username = auth_message.username.get();
            let password = auth_message.password.get();
            if username == "charlie" && password == "12345" {
                // Accept incoming connection
                server.accept_connection(&user_key);
            } else {
                // Reject incoming connection
                server.reject_connection(&user_key);
            }
        }
    }
}

pub fn connection_event<'world, 'state>(
    mut event_reader: EventReader<ConnectionEvent>,
    mut server: Server<'world, 'state, Protocol>,
    mut global: ResMut<Global>,
) {
    for event in event_reader.iter() {
        let ConnectionEvent(user_key) = event;
        let address = server
            .user_mut(&user_key)
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
            // Set Entity's owner to user
            .set_owner(&user_key)
            // return Entity id
            .id();

        global.user_to_prediction_map.insert(*user_key, entity);
    }
}

pub fn disconnection_event(
    mut event_reader: EventReader<DisconnectionEvent>,
    mut server: Server<Protocol>,
    mut global: ResMut<Global>,
) {
    for event in event_reader.iter() {
        let DisconnectionEvent(user_key, user) = event;
        info!("Naia Server disconnected from: {:?}", user.address);

        server.user_mut(&user_key).leave_room(&global.main_room_key);

        if let Some(entity) = global.user_to_prediction_map.remove(&user_key) {
            server
                .entity_mut(&entity)
                .leave_room(&global.main_room_key)
                .despawn();
        }
    }
}

pub fn command_event(
    mut event_reader: EventReader<CommandEvent<Protocol>>,
    mut q_player_position: Query<&mut Position>,
) {
    for event in event_reader.iter() {
        if let CommandEvent(_, entity, Protocol::KeyCommand(key_command)) = event {
            if let Ok(mut position) = q_player_position.get_mut(*entity) {
                shared_behavior::process_command(key_command, &mut position);
            }
        }
    }
}
