use bevy_ecs::{event::EventReader, system::ResMut};
use bevy_log::info;

use naia_bevy_server::{
    events::{AuthorizationEvent, ConnectionEvent, DisconnectionEvent, MessageEvent},
    shared::Random,
    Server,
};

use naia_bevy_demo_shared::{
    protocol::{Color, ColorValue, EntityAssignment, Position, Protocol},
    Channels,
};

use crate::resources::Global;

pub fn authorization_event(
    mut event_reader: EventReader<AuthorizationEvent<Protocol>>,
    mut server: Server<Protocol, Channels>,
) {
    for event in event_reader.iter() {
        if let AuthorizationEvent(user_key, Protocol::Auth(auth)) = event {
            if *auth.username == "charlie" && *auth.password == "12345" {
                // Accept incoming connection
                server.accept_connection(user_key);
            } else {
                // Reject incoming connection
                server.reject_connection(user_key);
            }
        }
    }
}

pub fn connection_event<'world, 'state>(
    mut event_reader: EventReader<ConnectionEvent>,
    mut global: ResMut<Global>,
    mut server: Server<'world, 'state, Protocol, Channels>,
) {
    for event in event_reader.iter() {
        let ConnectionEvent(user_key) = event;
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

        server.send_message(user_key, Channels::EntityAssignment, &assignment_message);
    }
}

pub fn disconnection_event(
    mut event_reader: EventReader<DisconnectionEvent>,
    mut global: ResMut<Global>,
    mut server: Server<Protocol, Channels>,
) {
    for event in event_reader.iter() {
        let DisconnectionEvent(user_key, user) = event;
        info!("Naia Server disconnected from: {:?}", user.address);

        if let Some(entity) = global.user_to_prediction_map.remove(user_key) {
            server
                .entity_mut(&entity)
                .leave_room(&global.main_room_key)
                .despawn();
        }
    }
}

pub fn receive_message_event(
    mut event_reader: EventReader<MessageEvent<Protocol, Channels>>,
    mut global: ResMut<Global>,
    server: Server<Protocol, Channels>,
) {
    for event in event_reader.iter() {
        if let MessageEvent(_user_key, Channels::PlayerCommand, Protocol::KeyCommand(key_command)) =
            event
        {
            if let Some(entity) = &key_command.entity.get(&server) {
                global
                    .player_last_command
                    .insert(*entity, key_command.clone());
            }
        }
    }
}
