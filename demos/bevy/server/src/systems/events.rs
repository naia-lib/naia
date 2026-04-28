use bevy_ecs::{
    message::Messages,
    system::{Commands, Query, ResMut},
};
use bevy_log::info;

use naia_bevy_server::{
    events::{
        AuthEvents, ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent,
        InsertComponentEvent, PublishEntityEvent, RemoveComponentEvent, RequestEvents,
        SpawnEntityEvent, TickEvent, UnpublishEntityEvent, UpdateComponentEvent,
    },
    CommandsExt, Random, ReplicationConfig, Server,
};

use naia_bevy_demo_shared::{
    behavior as shared_behavior,
    channels::{EntityAssignmentChannel, PlayerCommandChannel, RequestChannel},
    components::{Color, ColorValue, Position, Shape, ShapeValue},
    messages::{Auth, BasicRequest, BasicResponse, EntityAssignment, KeyCommand},
};

use crate::resources::Global;

pub fn auth_events(mut server: Server, mut event_reader: ResMut<Messages<AuthEvents>>) {
    for events in event_reader.drain() {
        for (user_key, auth) in events.read::<Auth>() {
            if auth.username == "charlie" && auth.password == "12345" {
                // Accept incoming connection
                info!("Naia Server accepted connection from: {:?}", user_key);
                server.accept_connection(&user_key);
            } else {
                // Reject incoming connection
                info!("Naia Server rejected connection from: {:?}", user_key);
                server.reject_connection(&user_key);
            }
        }
    }
}

pub fn connect_events(
    mut commands: Commands,
    mut server: Server,
    mut global: ResMut<Global>,
    mut event_reader: ResMut<Messages<ConnectEvent>>,
) {
    for ConnectEvent(user_key) in event_reader.drain() {
        let address = server
            .user_mut(&user_key)
            // Add User to the main Room
            .enter_room(&global.main_room_key)
            // Get User's address for logging
            .address();

        info!("Naia Server connected to: {}", address);

        // Spawn Entity to represent new player
        let entity = commands
            // Spawn new Entity
            .spawn_empty()
            // MUST call this to begin replication
            .enable_replication(&mut server)
            // Insert Position component
            .insert(Position::new(
                16 * ((Random::gen_range_u32(0, 40) as i16) - 20),
                16 * ((Random::gen_range_u32(0, 30) as i16) - 15),
            ))
            // Insert Color component
            .insert(Color::new(match server.users_count() % 4 {
                0 => ColorValue::Yellow,
                1 => ColorValue::Red,
                2 => ColorValue::Blue,
                _ => ColorValue::Green,
            }))
            // Insert Shape component
            .insert(Shape::new(ShapeValue::Square))
            // return Entity id
            .id();

        server.room_mut(&global.main_room_key).add_entity(&entity);

        global.user_to_square_map.insert(user_key, entity);
        global.square_to_user_map.insert(entity, user_key);

        // Send an Entity Assignment message to the User that owns the Square
        let mut assignment_message = EntityAssignment::new(true);
        assignment_message.entity.set(&server, &entity);

        server.send_message::<EntityAssignmentChannel, EntityAssignment>(
            &user_key,
            &assignment_message,
        );
    }
}

pub fn disconnect_events(
    mut commands: Commands,
    mut global: ResMut<Global>,
    mut event_reader: ResMut<Messages<DisconnectEvent>>,
) {
    for DisconnectEvent(user_key, address) in event_reader.drain() {
        info!("Naia Server disconnected from: {:?}", address);

        if let Some(entity) = global.user_to_square_map.remove(&user_key) {
            global.square_to_user_map.remove(&entity);
            commands.entity(entity).despawn();
        }
    }
}

pub fn error_events(mut event_reader: ResMut<Messages<ErrorEvent>>) {
    for ErrorEvent(error) in event_reader.drain() {
        info!("Naia Server Error: {:?}", error);
    }
}

pub fn tick_events(
    mut server: Server,
    mut position_query: Query<&mut Position>,
    mut global: ResMut<Global>,
    mut tick_reader: ResMut<Messages<TickEvent>>,
) {
    let mut has_ticked = false;

    for TickEvent(server_tick) in tick_reader.drain() {
        has_ticked = true;

        // All game logic should happen here, on a tick event

        let mut messages = server.receive_tick_buffer_messages(&server_tick);
        for (_user_key, key_command) in messages.read::<PlayerCommandChannel, KeyCommand>() {
            let Some(entity) = &key_command.entity.get(&server) else {
                continue;
            };
            let Ok(mut position) = position_query.get_mut(*entity) else {
                continue;
            };
            shared_behavior::process_command(&key_command, &mut position);
        }

        // Send a request to all clients
        if server_tick % 100 == 0 {
            for user_key in server.user_keys() {
                let request = BasicRequest::new("ServerRequest".to_string(), global.request_index);
                global.request_index = global.request_index.wrapping_add(1);
                let user = server.user(&user_key);
                info!(
                    "Server sending Request -> Client ({}): {:?}",
                    user.address(),
                    request
                );
                let Ok(response_key) =
                    server.send_request::<RequestChannel, _>(&user_key, &request)
                else {
                    info!("Failed to send request to user: {:?}", user_key);
                    continue;
                };
                global.response_keys.insert(response_key);
            }
        }
    }

    if has_ticked {
        // Add any newly-spawned or newly-joined entities to scope.
        // This demo unconditionally includes everything — use
        // scope_checks_all() instead if you need to exclude based on
        // game-state conditions (e.g. distance or visibility).
        for (_, user_key, entity) in server.scope_checks_pending() {
            server.user_scope_mut(&user_key).include(&entity);
        }
        server.mark_scope_checks_pending_handled();
    }
}

pub fn request_events(mut server: Server, mut event_reader: ResMut<Messages<RequestEvents>>) {
    for events in event_reader.drain() {
        for (user_key, response_send_key, request) in events.read::<RequestChannel, BasicRequest>()
        {
            let user = server.user(&user_key);
            info!(
                "Server received Request <- Client({}): {:?}",
                user.address(),
                request
            );
            let response = BasicResponse::new("ServerResponse".to_string(), request.index);
            info!(
                "Server sending Response -> Client({}): {:?}",
                user.address(),
                response
            );
            server.send_response(&response_send_key, &response);
        }
    }
}

pub fn response_events(mut server: Server, mut global: ResMut<Global>) {
    let mut finished_response_keys = Vec::new();
    for response_key in &global.response_keys {
        if let Some((user_key, response)) = server.receive_response(response_key) {
            let user = server.user(&user_key);
            info!(
                "Server received Response <- Client({:?}): {:?}",
                user.address(),
                response
            );
            finished_response_keys.push(response_key.clone());
        }
    }
    for response_key in finished_response_keys {
        global.response_keys.remove(&response_key);
    }
}

pub fn spawn_entity_events(
    mut commands: Commands,
    mut server: Server,
    global: ResMut<Global>,
    mut event_reader: ResMut<Messages<SpawnEntityEvent>>,
) {
    for SpawnEntityEvent(_user_key, client_entity) in event_reader.drain() {
        info!("spawned client entity, publish");

        // make public to other clients as well
        commands
            .entity(client_entity)
            .configure_replication(ReplicationConfig::public());

        server
            .room_mut(&global.main_room_key)
            .add_entity(&client_entity);
    }
}

pub fn despawn_entity_events(mut event_reader: ResMut<Messages<DespawnEntityEvent>>) {
    for DespawnEntityEvent(_, _) in event_reader.drain() {
        info!("despawned client entity");
    }
}

pub fn publish_entity_events(
    mut server: Server,
    global: ResMut<Global>,
    mut event_reader: ResMut<Messages<PublishEntityEvent>>,
) {
    for PublishEntityEvent(_user_key, client_entity) in event_reader.drain() {
        info!("client entity has been made public");

        // Add newly public entity to the main Room
        server
            .room_mut(&global.main_room_key)
            .add_entity(&client_entity);
    }
}

pub fn unpublish_entity_events(mut event_reader: ResMut<Messages<UnpublishEntityEvent>>) {
    for UnpublishEntityEvent(_user_key, _client_entity) in event_reader.drain() {
        info!("client entity has been unpublished");
    }
}

pub fn insert_component_events(
    mut position_events: ResMut<Messages<InsertComponentEvent<Position>>>,
    mut color_events: ResMut<Messages<InsertComponentEvent<Color>>>,
    mut shape_events: ResMut<Messages<InsertComponentEvent<Shape>>>,
) {
    for _event in position_events.drain() {
        info!("insert Position component into client entity");
    }
    for _event in color_events.drain() {
        info!("insert Color component into client entity");
    }
    for _event in shape_events.drain() {
        info!("insert Shape component into client entity");
    }
}

pub fn update_component_events(
    mut position_events: ResMut<Messages<UpdateComponentEvent<Position>>>,
) {
    for _events in position_events.drain() {
        // info!("update component in client entity");
    }
}

pub fn remove_component_events(
    mut position_events: ResMut<Messages<RemoveComponentEvent<Position>>>,
    mut color_events: ResMut<Messages<RemoveComponentEvent<Color>>>,
    mut shape_events: ResMut<Messages<RemoveComponentEvent<Shape>>>,
) {
    for _event in position_events.drain() {
        info!("remove Position component from client entity");
    }
    for _event in color_events.drain() {
        info!("remove Color component from client entity");
    }
    for _event in shape_events.drain() {
        info!("remove Shape component from client entity");
    }
}
