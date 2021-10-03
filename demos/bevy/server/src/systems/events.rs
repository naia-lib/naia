use bevy::prelude::*;

use naia_server::{Event as ServerEvent, Random, Ref, Server as NaiaServer};

use naia_bevy_server::{Entity, ServerCommands};

use naia_bevy_demo_shared::{
    behavior as shared_behavior,
    protocol::{Color, ColorValue, Position, Protocol},
};

use crate::resources::Global;

type Server = NaiaServer<Protocol, Entity>;

pub fn process_events(
    mut server: ResMut<Server>,
    mut server_commands: ResMut<ServerCommands>,
    mut events: EventReader<ServerEvent<Protocol, Entity>>,
    mut global: ResMut<Global>,
    q_position: Query<&Ref<Position>>,
) {
    for event in events.iter() {
        match event {
            ServerEvent::Authorization(user_key, Protocol::Auth(auth_ref)) => {
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
            ServerEvent::Connection(user_key) => {
                server.room_mut(&global.main_room_key).add_user(&user_key);
                let address = server.user(&user_key).address();
                info!("Naia Server connected to: {}", address);

                // Create new Square Entity
                let entity_key = server_commands.spawn().id();

                // Add Entity to main Room
                server
                    .room_mut(&global.main_room_key)
                    .add_entity(&entity_key);

                // Position component
                {
                    // create
                    let mut x = Random::gen_range_u32(0, 40) as i16;
                    let mut y = Random::gen_range_u32(0, 30) as i16;
                    x -= 20;
                    y -= 15;
                    x *= 16;
                    y *= 16;
                    let position_ref = Position::new(x, y);

                    // add to entity
                    server_commands.entity(&entity_key).insert(&position_ref);
                }

                // Color component
                {
                    // create
                    let color_value = match server.users_count() % 3 {
                        0 => ColorValue::Yellow,
                        1 => ColorValue::Red,
                        _ => ColorValue::Blue,
                    };
                    let color_ref = Color::new(color_value);

                    // add to entity
                    server_commands.entity(&entity_key).insert(&color_ref);
                }

                // Assign as Prediction to User
                server_commands.entity(&entity_key).set_owner(&user_key);
                global.user_to_prediction_map.insert(*user_key, entity_key);
            }
            ServerEvent::Disconnection(user_key, user) => {
                info!("Naia Server disconnected from: {:?}", user.address);

                server
                    .room_mut(&global.main_room_key)
                    .remove_user(&user_key);
                if let Some(naia_entity_key) = global.user_to_prediction_map.remove(&user_key) {
                    server
                        .room_mut(&global.main_room_key)
                        .remove_entity(&naia_entity_key);
                    server_commands.entity(&naia_entity_key).despawn();
                }
            }
            ServerEvent::Command(_, entity_key, Protocol::KeyCommand(key_command_ref)) => {
                if let Ok(position_ref) = q_position.get(**entity_key) {
                    shared_behavior::process_command(&key_command_ref, &position_ref);
                }
            }
            _ => {}
        }
    }
}
