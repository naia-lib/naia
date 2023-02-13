use bevy::{
    ecs::{
        event::EventReader,
        system::{Commands, Query, ResMut},
    },
    log::info,
    math::Vec2,
    render::color::Color as BevyColor,
    sprite::{Sprite, SpriteBundle},
    transform::components::Transform,
};

use naia_bevy_client::{
    events::{
        ConnectEvent, DisconnectEvent, InsertComponentEvent, MessageEvents, RejectEvent,
        SpawnEntityEvent, UpdateComponentEvent,
    },
    shared::{sequence_greater_than, Tick},
    Client, CommandsExt,
};

use naia_bevy_demo_shared::{
    behavior as shared_behavior,
    channels::EntityAssignmentChannel,
    components::{Color, ColorValue, Position},
    messages::EntityAssignment,
};

use crate::resources::{Global, OwnedEntity};

const SQUARE_SIZE: f32 = 32.0;

pub fn connect_events(mut event_reader: EventReader<ConnectEvent>, client: Client) {
    for _ in event_reader.iter() {
        if let Ok(server_address) = client.server_address() {
            info!("Client connected to: {}", server_address);
        }
    }
}

pub fn reject_events(mut event_reader: EventReader<RejectEvent>) {
    for _ in event_reader.iter() {
        info!("Client rejected from connecting to Server");
    }
}

pub fn disconnect_events(mut event_reader: EventReader<DisconnectEvent>) {
    for _ in event_reader.iter() {
        info!("Client disconnected from Server");
    }
}

pub fn message_events(
    mut event_reader: EventReader<MessageEvents>,
    mut local: Commands,
    mut global: ResMut<Global>,
    client: Client,
) {
    for events in event_reader.iter() {
        for message in events.read::<EntityAssignmentChannel, EntityAssignment>() {
            let assign = message.assign;

            let entity = message.entity.get(&client).unwrap();
            if assign {
                info!("gave ownership of entity");

                let prediction_entity = CommandsExt::duplicate_entity(&mut local, entity)
                    .insert(SpriteBundle {
                        sprite: Sprite {
                            custom_size: Some(Vec2::new(SQUARE_SIZE, SQUARE_SIZE)),
                            color: BevyColor::WHITE,
                            ..Default::default()
                        },
                        transform: Transform::from_xyz(0.0, 0.0, 0.0),
                        ..Default::default()
                    })
                    .id();

                global.owned_entity = Some(OwnedEntity::new(entity, prediction_entity));
            } else {
                let mut disowned: bool = false;
                if let Some(owned_entity) = &global.owned_entity {
                    if owned_entity.confirmed == entity {
                        local.entity(owned_entity.predicted).despawn();
                        disowned = true;
                    }
                }
                if disowned {
                    info!("removed ownership of entity");
                    global.owned_entity = None;
                }
            }
        }
    }
}

pub fn spawn_entity_events(mut event_reader: EventReader<SpawnEntityEvent>) {
    for event in event_reader.iter() {
        match event {
            SpawnEntityEvent(_entity) => {
                info!("spawned entity");
            }
        }
    }
}

pub fn insert_component_events(
    mut event_reader: EventReader<InsertComponentEvent>,
    mut local: Commands,
    color_query: Query<&Color>,
) {
    for InsertComponentEvent(entity, component_kind) in event_reader.iter() {
        if component_kind.is::<Color>() {
            if let Ok(color) = color_query.get(*entity) {
                info!("add color to entity");

                let color = {
                    match *color.value {
                        ColorValue::Red => BevyColor::RED,
                        ColorValue::Blue => BevyColor::BLUE,
                        ColorValue::Yellow => BevyColor::YELLOW,
                    }
                };

                local.entity(*entity).insert(SpriteBundle {
                    sprite: Sprite {
                        custom_size: Some(Vec2::new(SQUARE_SIZE, SQUARE_SIZE)),
                        color,
                        ..Default::default()
                    },
                    transform: Transform::from_xyz(0.0, 0.0, 0.0),
                    ..Default::default()
                });
            }
        }
    }
}

pub fn update_component_events(
    mut event_reader: EventReader<UpdateComponentEvent>,
    mut global: ResMut<Global>,
    mut position_query: Query<&mut Position>,
) {
    if let Some(owned_entity) = &global.owned_entity {
        let mut latest_tick: Option<Tick> = None;
        let server_entity = owned_entity.confirmed;
        let client_entity = owned_entity.predicted;

        for UpdateComponentEvent(server_tick, updated_entity, _) in event_reader.iter() {
            // If entity is owned
            if *updated_entity == server_entity {
                if let Some(last_tick) = &mut latest_tick {
                    if sequence_greater_than(*server_tick, *last_tick) {
                        *last_tick = *server_tick;
                    }
                } else {
                    latest_tick = Some(*server_tick);
                }
            }
        }

        if let Some(server_tick) = latest_tick {
            if let Ok([server_position, mut client_position]) =
                position_query.get_many_mut([server_entity, client_entity])
            {
                let replay_commands = global.command_history.replays(&server_tick);

                // set to authoritative state
                client_position.x.mirror(&server_position.x);
                client_position.y.mirror(&server_position.y);

                // Replay all stored commands
                for (_command_tick, command) in replay_commands {
                    shared_behavior::process_command(&command, &mut client_position);
                }
            }
        }
    }
}
