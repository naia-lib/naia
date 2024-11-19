use std::default::Default;

use bevy::{
    color::LinearRgba,
    log::info,
    prelude::{
        Color as BevyColor, Commands, EventReader, Query, Res, ResMut, Sprite, SpriteBundle,
        Transform, Vec2,
    },
    sprite::MaterialMesh2dBundle,
};

use naia_bevy_client::{
    events::{
        ClientTickEvent, ConnectEvent, DespawnEntityEvent, DisconnectEvent, InsertComponentEvents,
        MessageEvents, PublishEntityEvent, RejectEvent, RemoveComponentEvents, RequestEvents,
        SpawnEntityEvent, UnpublishEntityEvent, UpdateComponentEvents,
    },
    sequence_greater_than, Client, CommandsExt, Random, Replicate, Tick,
};

use naia_bevy_demo_shared::{
    behavior as shared_behavior,
    channels::{EntityAssignmentChannel, PlayerCommandChannel, RequestChannel},
    components::{Color, ColorValue, Position, Shape, ShapeValue},
    messages::{BasicRequest, BasicResponse, EntityAssignment, KeyCommand},
};

use crate::{
    app::Main,
    components::{Confirmed, Interp, LocalCursor, Predicted},
    resources::{Global, OwnedEntity},
};

const SQUARE_SIZE: f32 = 32.0;

pub fn connect_events(
    mut commands: Commands,
    mut client: Client<Main>,
    mut global: ResMut<Global>,
    mut event_reader: EventReader<ConnectEvent<Main>>,
) {
    for _ in event_reader.read() {
        let Ok(server_address) = client.server_address() else {
            panic!("Shouldn't happen");
        };
        info!("Client connected to: {}", server_address);

        // Create entity for Client-authoritative Cursor

        // Spawn Cursor Entity
        let cursor_entity = commands
            // Spawn new Square Entity
            .spawn_empty()
            // MUST call this to begin replication
            .enable_replication(&mut client)
            // make Entity Public, which means it will be visibile to other Clients
            //.configure_replication(&mut client, ReplicationConfig::Public)
            // Insert Position component
            .insert(Position::new(
                16 * ((Random::gen_range_u32(0, 40) as i16) - 20),
                16 * ((Random::gen_range_u32(0, 30) as i16) - 15),
            ))
            // Insert Shape component
            .insert(Shape::new(ShapeValue::Circle))
            // Insert Cursor marker component
            .insert(LocalCursor)
            // return Entity id
            .id();

        global.cursor_entity = Some(cursor_entity);
    }
}

pub fn reject_events(mut event_reader: EventReader<RejectEvent<Main>>) {
    for _ in event_reader.read() {
        info!("Client rejected from connecting to Server");
    }
}

pub fn disconnect_events(mut event_reader: EventReader<DisconnectEvent<Main>>) {
    for _ in event_reader.read() {
        info!("Client disconnected from Server");
    }
}

pub fn message_events(
    mut commands: Commands,
    client: Client<Main>,
    mut global: ResMut<Global>,
    mut event_reader: EventReader<MessageEvents<Main>>,
    position_query: Query<&Position>,
    color_query: Query<&Color>,
) {
    for events in event_reader.read() {
        for message in events.read::<EntityAssignmentChannel, EntityAssignment>() {
            let assign = message.assign;

            let entity = message.entity.get(&client).unwrap();
            if assign {
                info!("gave ownership of entity");

                // Here we create a local copy of the Player entity, to use for client-side prediction
                if let Ok(position) = position_query.get(entity) {
                    let prediction_entity = commands.entity(entity).local_duplicate(); // copies all Replicate components as well
                    commands
                        .entity(prediction_entity)
                        .insert(SpriteBundle {
                            sprite: Sprite {
                                custom_size: Some(Vec2::new(SQUARE_SIZE, SQUARE_SIZE)),
                                color: BevyColor::WHITE,
                                ..Default::default()
                            },
                            transform: Transform::from_xyz(0.0, 0.0, 1.0),
                            ..Default::default()
                        })
                        // insert interpolation component
                        .insert(Interp::new(*position.x, *position.y))
                        // mark as predicted
                        .insert(Predicted);

                    global.owned_entity = Some(OwnedEntity::new(entity, prediction_entity));
                }
                // Now that we know the Color of the player, we assign it to our Cursor
                if let Ok(color) = color_query.get(entity) {
                    if let Some(cursor_entity) = global.cursor_entity {
                        // Add Color to cursor entity
                        commands.entity(cursor_entity).insert(color.clone());

                        // Insert SpriteBundle locally only
                        let color_handle = {
                            match *color.value {
                                ColorValue::Red => &global.red,
                                ColorValue::Blue => &global.blue,
                                ColorValue::Yellow => &global.yellow,
                                ColorValue::Green => &global.green,
                                ColorValue::White => &global.white,
                                ColorValue::Purple => &global.purple,
                                ColorValue::Orange => &global.orange,
                                ColorValue::Aqua => &global.aqua,
                            }
                        };
                        commands.entity(cursor_entity).insert(MaterialMesh2dBundle {
                            mesh: global.circle.clone().into(),
                            material: color_handle.clone(),
                            transform: Transform::from_xyz(0.0, 0.0, 0.0),
                            ..Default::default()
                        });
                        info!("assigned color to cursor");
                    }
                }
            } else {
                let mut disowned: bool = false;
                if let Some(owned_entity) = &global.owned_entity {
                    if owned_entity.confirmed == entity {
                        commands.entity(owned_entity.predicted).despawn();
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

pub fn request_events(
    mut client: Client<Main>,
    mut event_reader: EventReader<RequestEvents<Main>>,
) {
    for events in event_reader.read() {
        for (response_send_key, request) in events.read::<RequestChannel, BasicRequest>() {
            info!("Client received Request <- Server: {:?}", request);
            let response = BasicResponse::new("ClientResponse".to_string(), request.index);
            info!("Client sending Response -> Server: {:?}", response);
            client.send_response(&response_send_key, &response);
        }
    }
}

pub fn response_events(mut client: Client<Main>, mut global: ResMut<Global>) {
    let mut finished_response_keys = Vec::new();
    for response_key in &global.response_keys {
        if let Some(response) = client.receive_response(response_key) {
            info!("Client received Response <- Server: {:?}", response);
            finished_response_keys.push(response_key.clone());
        }
    }
    for response_key in finished_response_keys {
        global.response_keys.remove(&response_key);
    }
}

pub fn spawn_entity_events(mut event_reader: EventReader<SpawnEntityEvent<Main>>) {
    for _event in event_reader.read() {
        info!("spawned entity");
    }
}

pub fn despawn_entity_events(mut event_reader: EventReader<DespawnEntityEvent<Main>>) {
    for _event in event_reader.read() {
        info!("despawned entity");
    }
}

pub fn publish_entity_events(mut event_reader: EventReader<PublishEntityEvent<Main>>) {
    for _event in event_reader.read() {
        info!("client demo: publish entity event");
    }
}

pub fn unpublish_entity_events(mut event_reader: EventReader<UnpublishEntityEvent<Main>>) {
    for _event in event_reader.read() {
        info!("client demo: unpublish entity event");
    }
}

pub fn insert_component_events(
    mut commands: Commands,
    mut event_reader: EventReader<InsertComponentEvents<Main>>,
    global: Res<Global>,
    sprite_query: Query<(&Shape, &Color)>,
    position_query: Query<&Position>,
) {
    for events in event_reader.read() {
        for entity in events.read::<Color>() {
            // When we receive a replicated Color component for a given Entity,
            // use that value to also insert a local-only SpriteBundle component into this entity
            info!("add Color Component to entity");

            if let Ok((shape, color)) = sprite_query.get(entity) {
                match *shape.value {
                    // Square
                    ShapeValue::Square => {
                        let color = {
                            match *color.value {
                                ColorValue::Red => BevyColor::LinearRgba(LinearRgba::RED),
                                ColorValue::Blue => BevyColor::LinearRgba(LinearRgba::BLUE),
                                ColorValue::Yellow => {
                                    BevyColor::LinearRgba(LinearRgba::rgb(1.0, 1.0, 0.0))
                                }
                                ColorValue::Green => BevyColor::LinearRgba(LinearRgba::GREEN),
                                ColorValue::White => BevyColor::LinearRgba(LinearRgba::WHITE),
                                ColorValue::Purple => {
                                    BevyColor::LinearRgba(LinearRgba::rgb(1.0, 0.0, 1.0))
                                }
                                ColorValue::Orange => {
                                    BevyColor::LinearRgba(LinearRgba::rgb(1.0, 0.5, 0.0))
                                }
                                ColorValue::Aqua => {
                                    BevyColor::LinearRgba(LinearRgba::rgb(0.0, 1.0, 1.0))
                                }
                            }
                        };

                        commands
                            .entity(entity)
                            .insert(SpriteBundle {
                                sprite: Sprite {
                                    custom_size: Some(Vec2::new(SQUARE_SIZE, SQUARE_SIZE)),
                                    color,
                                    ..Default::default()
                                },
                                transform: Transform::from_xyz(0.0, 0.0, 0.0),
                                ..Default::default()
                            })
                            // mark as confirmed
                            .insert(Confirmed);
                    }
                    // Circle
                    ShapeValue::Circle => {
                        let handle = {
                            match *color.value {
                                ColorValue::Red => &global.red,
                                ColorValue::Blue => &global.blue,
                                ColorValue::Yellow => &global.yellow,
                                ColorValue::Green => &global.green,
                                ColorValue::White => &global.white,
                                ColorValue::Purple => &global.purple,
                                ColorValue::Orange => &global.orange,
                                ColorValue::Aqua => &global.aqua,
                            }
                        };
                        commands
                            .entity(entity)
                            .insert(MaterialMesh2dBundle {
                                mesh: global.circle.clone().into(),
                                material: handle.clone(),
                                transform: Transform::from_xyz(0.0, 0.0, 0.0),
                                ..Default::default()
                            })
                            // mark as confirmed
                            .insert(Confirmed);
                    }
                }
            } else {
                panic!("spritequery failed!");
            }
        }
        for entity in events.read::<Position>() {
            info!("add Position Component to entity");
            if let Ok(position) = position_query.get(entity) {
                // initialize interpolation
                commands
                    .entity(entity)
                    .insert(Interp::new(*position.x, *position.y));
            }
        }
        for _entity in events.read::<Shape>() {
            info!("add Shape Component to entity");
        }
    }
}

pub fn update_component_events(
    mut global: ResMut<Global>,
    mut event_reader: EventReader<UpdateComponentEvents<Main>>,
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

        for events in event_reader.read() {
            // Update square position
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
                // Set to authoritative state
                client_position.mirror(&*server_position);

                // Replay all stored commands

                // TODO: why is it necessary to subtract 1 Tick here?
                // it's not like this in the Macroquad demo
                let modified_server_tick = server_tick.wrapping_sub(1);

                let replay_commands = global.command_history.replays(&modified_server_tick);
                for (_command_tick, command) in replay_commands {
                    shared_behavior::process_command(&command, &mut client_position);
                }
            }
        }
    }
}

pub fn remove_component_events(mut event_reader: EventReader<RemoveComponentEvents<Main>>) {
    for events in event_reader.read() {
        for (_entity, _component) in events.read::<Position>() {
            info!("removed Position component from entity");
        }
        for (_entity, _component) in events.read::<Color>() {
            info!("removed Color component from entity");
        }
    }
}

pub fn tick_events(
    mut client: Client<Main>,
    mut global: ResMut<Global>,
    mut tick_reader: EventReader<ClientTickEvent<Main>>,
    mut position_query: Query<&mut Position>,
) {
    let Some(predicted_entity) = global
        .owned_entity
        .as_ref()
        .map(|owned_entity| owned_entity.predicted)
    else {
        // No owned Entity
        return;
    };

    let Some(command) = global.queued_command.take() else {
        return;
    };

    for event in tick_reader.read() {
        let client_tick = event.tick;

        // Send a request to server
        if client_tick % 100 == 0 {
            let request = BasicRequest::new("ClientRequest".to_string(), global.request_index);
            global.request_index = global.request_index.wrapping_add(1);

            info!("Client sending Request -> Server: {:?}", request);
            let Ok(response_key) = client.send_request::<RequestChannel, _>(&request) else {
                info!("Failed to send request to server");
                return;
            };
            global.response_keys.insert(response_key);
        }

        // Command History
        if !global.command_history.can_insert(&client_tick) {
            // History is full
            continue;
        }

        // Record command
        global.command_history.insert(client_tick, command.clone());

        // Send command
        client.send_tick_buffer_message::<PlayerCommandChannel, KeyCommand>(&client_tick, &command);

        if let Ok(mut position) = position_query.get_mut(predicted_entity) {
            // Apply command
            shared_behavior::process_command(&command, &mut position);
        }
    }
}
