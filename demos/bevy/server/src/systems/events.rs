use bevy_ecs::{event::EventReader, system::ResMut};
use bevy_log::info;

use naia_bevy_server::{
    events::{AuthEvents, ConnectEvent, DisconnectEvent, ErrorEvent, MessageEvents},
    shared::Random,
    Server,
};

use naia_bevy_demo_shared::{
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

pub fn connect_events<'world, 'state>(
    mut event_reader: EventReader<ConnectEvent>,
    mut global: ResMut<Global>,
    mut server: Server<'world, 'state>,
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

        global.user_to_prediction_map.insert(*user_key, entity);

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

        if let Some(entity) = global.user_to_prediction_map.remove(user_key) {
            server
                .entity_mut(&entity)
                .leave_room(&global.main_room_key)
                .despawn();
        }
    }
}

pub fn message_events(
    mut event_reader: EventReader<MessageEvents>,
    mut global: ResMut<Global>,
    server: Server,
) {
    for events in event_reader.iter() {
        for (_user_key, key_command) in events.read::<PlayerCommandChannel, KeyCommand>() {
            if let Some(entity) = &key_command.entity.get(&server) {
                global
                    .player_last_command
                    .insert(*entity, key_command.clone());
            }
        }
    }
}

pub fn error_events(mut event_reader: EventReader<ErrorEvent>) {
    for ErrorEvent(error) in event_reader.iter() {
        info!("Naia Server Error: {:?}", error);
    }
}
