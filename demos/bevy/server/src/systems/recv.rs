use bevy::{
    ecs::system::{Query, ResMut},
    log::info,
};

use naia_bevy_server::{Event, Random, Ref, Server};

use naia_bevy_demo_shared::{
    behavior as shared_behavior,
    protocol::{Color, ColorValue, Position, Protocol},
};

use crate::resources::Global;

pub fn receive_events(
    mut server: Server<Protocol>,
    mut global: ResMut<Global>,
    q_position: Query<&Ref<Position>>,
) {
    for event in server.receive() {
        match event {
            Ok(Event::Authorization(user_key, Protocol::Auth(auth_ref))) => {
                let auth_message = auth_ref.borrow();
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
            Ok(Event::Connection(user_key)) => {
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
                    .insert(&position)
                    // Insert Color component
                    .insert(&color)
                    // Set Entity's owner to user
                    .set_owner(&user_key)
                    // return Entity id
                    .id();

                global.user_to_prediction_map.insert(user_key, entity);
            }
            Ok(Event::Disconnection(user_key, user)) => {
                info!("Naia Server disconnected from: {:?}", user.address);

                server.user_mut(&user_key).leave_room(&global.main_room_key);

                if let Some(entity) = global.user_to_prediction_map.remove(&user_key) {
                    server
                        .entity_mut(&entity)
                        .leave_room(&global.main_room_key)
                        .despawn();
                }
            }
            Ok(Event::Command(_, entity, Protocol::KeyCommand(key_command_ref))) => {
                if let Ok(position_ref) = q_position.get(*entity) {
                    shared_behavior::process_command(&key_command_ref, &position_ref);
                }
            }
            Ok(Event::Tick) => {
                server.tick_start();
            }
            _ => {}
        }
    }
}
