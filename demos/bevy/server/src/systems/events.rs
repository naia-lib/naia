use bevy_ecs::{
    event::EventReader,
    system::{Commands, Query, ResMut},
};
use bevy_log::info;

use naia_bevy_server::{
    events::{
        AuthEvents, ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent,
        InsertComponentEvents, PublishEntityEvent, RemoveComponentEvents, SpawnEntityEvent,
        TickEvent, UpdateComponentEvents,
    },
    CommandsExt, Random, ReplicationConfig, Server,
};

use naia_bevy_demo_shared::{
    behavior as shared_behavior,
    channels::{EntityAssignmentChannel, PlayerCommandChannel},
    components::{Color, ColorValue, Position, Shape, ShapeValue},
    messages::{Auth, EntityAssignment, KeyCommand},
};
use naia_bevy_server::events::UnpublishEntityEvent;

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

        global.user_to_square_map.insert(*user_key, entity);
        global.square_to_user_map.insert(entity, *user_key);

        // Send an Entity Assignment message to the User that owns the Square
        let mut assignment_message = EntityAssignment::new(true);
        assignment_message.entity.set(&server, &entity);

        server.send_message::<EntityAssignmentChannel, EntityAssignment>(
            user_key,
            &assignment_message,
        );

        // Create a new delegated Entity
        let delegated_entity = commands
            // Spawn new Entity
            .spawn_empty()
            // MUST call this to begin replication
            .enable_replication(&mut server)
            .configure_replication(ReplicationConfig::Delegated)
            // Insert Position component
            .insert(Position::new(
                16 * ((Random::gen_range_u32(0, 40) as i16) - 20),
                16 * ((Random::gen_range_u32(0, 30) as i16) - 15),
            ))
            // return Entity id
            .id();
        server.room_mut(&global.main_room_key).add_entity(&delegated_entity);
    }
}

pub fn disconnect_events(
    mut commands: Commands,
    mut global: ResMut<Global>,
    mut event_reader: EventReader<DisconnectEvent>,
) {
    for DisconnectEvent(user_key, user) in event_reader.iter() {
        info!("Naia Server disconnected from: {:?}", user.address);

        if let Some(entity) = global.user_to_square_map.remove(user_key) {
            global.square_to_user_map.remove(&entity);
            commands.entity(entity).despawn();
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
    }
}

pub fn spawn_entity_events(
    mut commands: Commands,
    mut server: Server,
    global: ResMut<Global>,
    mut event_reader: EventReader<SpawnEntityEvent>,
) {
    for SpawnEntityEvent(_user_key, client_entity) in event_reader.iter() {
        info!("spawned client entity, publish");

        // make public to other clients as well
        commands
            .entity(*client_entity)
            .configure_replication(ReplicationConfig::Public);

        server
            .room_mut(&global.main_room_key)
            .add_entity(client_entity);
    }
}

pub fn despawn_entity_events(mut event_reader: EventReader<DespawnEntityEvent>) {
    for DespawnEntityEvent(_, _) in event_reader.iter() {
        info!("despawned client entity");
    }
}

pub fn publish_entity_events(
    mut server: Server,
    global: ResMut<Global>,
    mut event_reader: EventReader<PublishEntityEvent>,
) {
    for PublishEntityEvent(_user_key, client_entity) in event_reader.iter() {
        info!("client entity has been made public");

        // Add newly public entity to the main Room
        server
            .room_mut(&global.main_room_key)
            .add_entity(client_entity);
    }
}

pub fn unpublish_entity_events(mut event_reader: EventReader<UnpublishEntityEvent>) {
    for UnpublishEntityEvent(_user_key, _client_entity) in event_reader.iter() {
        info!("client entity has been unpublished");
    }
}

pub fn insert_component_events(mut event_reader: EventReader<InsertComponentEvents>) {
    for events in event_reader.iter() {
        for (_user_key, _client_entity) in events.read::<Position>() {
            info!("insert Position component into client entity");
        }
        for (_user_key, _client_entity) in events.read::<Color>() {
            info!("insert Color component into client entity");
        }
        for (_user_key, _client_entity) in events.read::<Shape>() {
            info!("insert Shape component into client entity");
        }
    }
}

pub fn update_component_events(mut event_reader: EventReader<UpdateComponentEvents>) {
    for events in event_reader.iter() {
        for (_user_key, _client_entity) in events.read::<Position>() {
            // info!("update component in client entity");
        }
    }
}

pub fn remove_component_events(mut event_reader: EventReader<RemoveComponentEvents>) {
    for events in event_reader.iter() {
        for (_user_key, _entity, _component) in events.read::<Position>() {
            info!("removed Position component from client entity");
        }
        for (_user_key, _entity, _component) in events.read::<Color>() {
            info!("removed Color component from client entity");
        }
        for (_user_key, _entity, _component) in events.read::<Shape>() {
            info!("removed Shape component from client entity");
        }
    }
}
