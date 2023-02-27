use bevy_ecs::{
    event::EventReader,
    system::{Commands, Query, ResMut},
};
use bevy_log::info;
use bevy_math::Vec2;
use bevy_render::color::Color as BevyColor;
use bevy_sprite::{Sprite, SpriteBundle};
use bevy_transform::components::Transform;

use naia_bevy_client::{
    events::{
        ClientTickEvent, ConnectEvent, DisconnectEvent, InsertComponentEvents, MessageEvents,
        RejectEvent, SpawnEntityEvent, UpdateComponentEvents,
    },
    sequence_greater_than, Client, CommandsExt, Tick,
};

use naia_bevy_demo_shared::{
    behavior as shared_behavior,
    channels::{EntityAssignmentChannel, PlayerCommandChannel},
    components::{Color, ColorValue, Position},
    messages::{EntityAssignment, KeyCommand},
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

            let hidden_string = message.big_thing;
            info!("Here's the text: {hidden_string}");

            let assign = message.assign;

            let entity = message.entity.get(&client).unwrap();
            if assign {
                info!("gave ownership of entity");

                // Here we create a local copy of the Player entity, to use for client-side prediction
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
    for SpawnEntityEvent(_entity) in event_reader.iter() {
        info!("spawned entity");
    }
}

pub fn insert_component_events(
    mut event_reader: EventReader<InsertComponentEvents>,
    mut local: Commands,
    color_query: Query<&Color>,
) {
    for events in event_reader.iter() {
        for entity in events.read::<Color>() {
            if let Ok(color) = color_query.get(entity) {
                // When we receive a replicated Color component for a given Entity,
                // use that value to also insert a local-only SpriteBundle component into this entity
                info!("add Color Component to entity");

                let color = {
                    match *color.value {
                        ColorValue::Red => BevyColor::RED,
                        ColorValue::Blue => BevyColor::BLUE,
                        ColorValue::Yellow => BevyColor::YELLOW,
                    }
                };

                local.entity(entity).insert(SpriteBundle {
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
    mut event_reader: EventReader<UpdateComponentEvents>,
    mut global: ResMut<Global>,
    mut position_query: Query<&mut Position>,
) {
    // When we receive a new Position update for the Player's Entity,
    // we must ensure the Client-side Prediction also remains in-sync
    // So we roll the Prediction back to the authoritative Server state
    // and then execute all Player Commands since that tick, using the CommandHistory helper struct
    if let Some(owned_entity) = &global.owned_entity {
        let mut latest_tick: Option<Tick> = None;
        let server_entity = owned_entity.confirmed;
        let client_entity = owned_entity.predicted;

        for events in event_reader.iter() {
            for (server_tick, updated_entity) in events.read::<Position>() {
                // If entity is owned
                if updated_entity == server_entity {
                    if let Some(last_tick) = &mut latest_tick {
                        if sequence_greater_than(server_tick, *last_tick) {
                            *last_tick = server_tick;
                        }
                    } else {
                        latest_tick = Some(server_tick);
                    }
                }
            }
        }

        if let Some(server_tick) = latest_tick {
            if let Ok([server_position, mut client_position]) =
                position_query.get_many_mut([server_entity, client_entity])
            {
                let replay_commands = global.command_history.replays(&server_tick);

                // Set to authoritative state
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

pub fn tick_events(
    mut tick_reader: EventReader<ClientTickEvent>,
    mut global: ResMut<Global>,
    mut client: Client,
    mut position_query: Query<&mut Position>,
) {
    if !client.is_connected() {
        return;
    }
    let Some(command) = global.queued_command.take() else {
        return;
    };

    let Some(predicted_entity) = global
        .owned_entity
        .as_ref()
        .map(|owned_entity| owned_entity.predicted) else {
        // No owned Entity
        return;
    };

    for ClientTickEvent(tick) in tick_reader.iter() {
        global.last_client_tick = *tick;

        //All game logic should happen here, on a tick event
        if !global.command_history.can_insert(tick) {
            // History is full
            continue;
        }

        // Record command
        global.command_history.insert(*tick, command.clone());

        // Send command
        client.send_tick_buffer_message::<PlayerCommandChannel, KeyCommand>(tick, &command);

        // Apply command
        if let Ok(mut position) = position_query.get_mut(predicted_entity) {
            shared_behavior::process_command(&command, &mut position);
        }
    }
}
