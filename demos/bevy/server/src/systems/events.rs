use bevy_ecs::{
    event::EventReader,
    system::{Commands, Query, ResMut},
    prelude::{Entity, Local, Res}
};
use bevy_log::info;

use naia_bevy_server::{
    events::{
        AuthEvents, ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent,
        InsertComponentEvents, RemoveComponentEvents, SpawnEntityEvent, TickEvent,
        UpdateComponentEvents,
    },
    CommandsExt, Random, Server,
};

use naia_bevy_demo_shared::{
    behavior as shared_behavior,
    channels::{EntityAssignmentChannel, PlayerCommandChannel},
    components::{Color, ColorValue, Position, Relation, Shape, ShapeValue},
    messages::{Auth, EntityAssignment, KeyCommand},
};
use naia_bevy_demo_shared::components::Baseline;

use crate::resources::Global;

pub fn auth_events(mut server: Server, mut event_reader: EventReader<AuthEvents>) {
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
    mut commands: Commands,
    mut server: Server,
    mut global: ResMut<Global>,
    mut event_reader: EventReader<ConnectEvent>,
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
                2 => ColorValue::Blue,
                _ => ColorValue::Green,
            };
            Color::new(color_value)
        };

        // Shape component
        let shape = Shape::new(ShapeValue::Square);

        // Relation component
        let mut relation = Relation::new();
        relation.entity.set(&server, &global.server_baseline);

        // Spawn entity
        let entity = commands
            // Spawn new Entity
            .spawn_empty()
            // MUST call this to begin replication
            .enable_replication(&mut server)
            // Insert Position component
            .insert(position)
            // Insert Color component
            .insert(color)
            // Insert Shape component
            .insert(shape)
            // Insert Relation component
            .insert(relation)
            // return Entity id
            .id();

        server.room_mut(&global.main_room_key).add_entity(&entity);

        global.user_to_square_map.insert(*user_key, entity);
        global.square_to_user_map.insert(entity, *user_key);

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
    mut commands: Commands,
    mut server: Server,
    mut global: ResMut<Global>,
    mut event_reader: EventReader<DisconnectEvent>,
) {
    for DisconnectEvent(user_key, user) in event_reader.iter() {
        info!("Naia Server disconnected from: {:?}", user.address);

        if let Some(entity) = global.user_to_square_map.remove(user_key) {
            global.square_to_user_map.remove(&entity);
            commands.entity(entity).despawn();
            server
                .room_mut(&global.main_room_key)
                .remove_entity(&entity);
        }
        if let Some(client_entity) = global.user_to_cursor_map.remove(user_key) {
            if let Some(server_entity) = global.client_to_server_cursor_map.remove(&client_entity) {
                commands.entity(server_entity).despawn();
                server
                    .room_mut(&global.main_room_key)
                    .remove_entity(&server_entity);
            }
        }
        global.client_baselines.remove(user_key);
    }
}

pub fn error_events(mut event_reader: EventReader<ErrorEvent>) {
    for ErrorEvent(error) in event_reader.iter() {
        info!("Naia Server Error: {:?}", error);
    }
}

pub fn tick_events(
    mut server: Server,
    global: Res<Global>,
    mut position_query: Query<&mut Position>,
    mut tick_reader: EventReader<TickEvent>,
    mut timer: Local<u16>,
    mut relation_query: Query<(Entity, &mut Relation)>,
) {
    let mut has_ticked = false;

    for TickEvent(server_tick) in tick_reader.iter() {
        has_ticked = true;

        // All game logic should happen here, on a tick event

        let mut messages = server.receive_tick_buffer_messages(server_tick);
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

        // Move relations around
        *timer += 1;
        if *timer >= 30 {
            *timer = 0;
            info!("Timer went off!");
            for (square_entity, mut relation) in relation_query.iter_mut() {
                info!(" - have entity");
                if let Some(relation_entity) = relation.entity.get(&server) {
                    info!(" - have relation entity");
                    if let Some(user_key) = global.square_to_user_map.get(&square_entity) {
                        info!(" - have user key");
                        if relation_entity == global.server_baseline {
                            info!(" - relation is server baseline, switching to client baseline");
                            // switch to client's baseline
                            if let Some(client_baseline) = global.client_baselines.get(&user_key) {
                                relation.entity.set(&server, client_baseline);
                            }
                        } else {
                            info!(" - relation is client baseline, switching to server baseline");
                            // switch to server's baseline
                            relation.entity.set(&server, &global.server_baseline);
                        }
                    }
                }
            }
        }
    }
}

pub fn spawn_entity_events(mut event_reader: EventReader<SpawnEntityEvent>) {
    for SpawnEntityEvent(_, _) in event_reader.iter() {
        info!("spawned client entity");
    }
}

pub fn despawn_entity_events(mut event_reader: EventReader<DespawnEntityEvent>) {
    for DespawnEntityEvent(_, _) in event_reader.iter() {
        info!("despawned client entity");
    }
}

pub fn insert_component_events(
    mut commands: Commands,
    mut server: Server,
    mut global: ResMut<Global>,
    mut event_reader: EventReader<InsertComponentEvents>,
    position_query: Query<&Position>,
) {
    for events in event_reader.iter() {
        for (user_key, client_entity) in events.read::<Position>() {
            info!("insert component into client entity");

            if let Ok(client_position) = position_query.get(client_entity) {
                // New Position Component
                let server_position = Position::new(*client_position.x, *client_position.y);

                // New Color component
                let color = {
                    let color_value = match server.users_count() % 3 {
                        0 => ColorValue::Yellow,
                        1 => ColorValue::Red,
                        2 => ColorValue::Blue,
                        _ => ColorValue::Green,
                    };
                    Color::new(color_value)
                };

                // New Shape component
                let shape = Shape::new(ShapeValue::Circle);

                // Spawn entity
                let server_entity = commands
                    // Spawn new Square Entity
                    .spawn_empty()
                    // MUST call this to begin replication
                    .enable_replication(&mut server)
                    // Insert Position component
                    .insert(server_position)
                    // Insert Color component
                    .insert(color)
                    // Insert Shape component
                    .insert(shape)
                    // return Entity id
                    .id();

                server
                    .room_mut(&global.main_room_key)
                    .add_entity(&server_entity);

                global.user_to_cursor_map.insert(user_key, client_entity);
                global
                    .client_to_server_cursor_map
                    .insert(client_entity, server_entity);
            }
        }
        for (user_key, client_entity) in events.read::<Baseline>() {
            global.client_baselines.insert(user_key, client_entity);
        }
    }
}

pub fn update_component_events(
    global: ResMut<Global>,
    mut event_reader: EventReader<UpdateComponentEvents>,
    mut position_query: Query<&mut Position>,
) {
    for events in event_reader.iter() {
        for (_user_key, client_entity) in events.read::<Position>() {
            if let Some(server_entity) = global.client_to_server_cursor_map.get(&client_entity) {
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

pub fn remove_component_events(mut event_reader: EventReader<RemoveComponentEvents>) {
    for events in event_reader.iter() {
        for (_user_key, _entity, _component) in events.read::<Position>() {
            info!("removed Position component from client entity");
        }
    }
}
