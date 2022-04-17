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
    events::{InsertComponentEvent, MessageEvent, SpawnEntityEvent},
    Client,
};

use naia_bevy_demo_shared::{
    protocol::{Color, ColorValue, Protocol, ProtocolKind},
    Channels,
};

use crate::resources::{Global, OwnedEntity};

const SQUARE_SIZE: f32 = 32.0;

pub fn connect_event(client: Client<Protocol, Channels>) {
    info!("Client connected to: {}", client.server_address());
}

pub fn disconnect_event(client: Client<Protocol, Channels>) {
    info!("Client disconnected from: {}", client.server_address());
}

pub fn receive_message_event(
    mut event_reader: EventReader<MessageEvent<Protocol, Channels>>,
    mut local: Commands,
    client: Client<Protocol, Channels>,
    global: ResMut<Global>)
{
    for event in event_reader.iter() {
        match event {
            MessageEvent(Channels::EntityAssignment, Protocol::EntityAssignment(message)) => {
                let assign = *message.assign;

                let entity = message.entity.get(&client).unwrap();
                if assign {
                    info!("gave ownership of entity");
                    let prediction_entity = local.spawn().id();
                    let components = client.entity(entity).

                    for component_kind in self.component_kinds(&entity) {
                        let mut component_copy_opt: Option<P> = None;
                        if let Some(component) =
                        self.component_of_kind(&entity, &component_kind)
                        {
                            component_copy_opt = Some(component.protocol_copy());
                        }
                        if let Some(component_copy) = component_copy_opt {
                            component_copy
                                .extract_and_insert(&new_entity, self);
                        }
                    }

                    global.owned_entity = Some(OwnedEntity::new(entity, prediction_entity));
                } else {
                    let mut disowned: bool = false;
                    if let Some(owned_entity) = &global.owned_entity {
                        if owned_entity.confirmed == entity {
                            self.world
                                .proxy_mut()
                                .despawn_entity(&owned_entity.predicted);
                            disowned = true;
                        }
                    }
                    if disowned {
                        info!("removed ownership of entity");
                        self.owned_entity = None;
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn spawn_entity_event(mut _local: Commands, mut event_reader: EventReader<SpawnEntityEvent>) {
    for event in event_reader.iter() {
        match event {
            SpawnEntityEvent(_entity) => {
                info!("spawned entity");
            }
        }
    }
}

pub fn insert_component_event(
    mut local: Commands,
    mut event_reader: EventReader<InsertComponentEvent<ProtocolKind>>,
    color_query: Query<&Color>,
) {
    for event in event_reader.iter() {
        match event {
            InsertComponentEvent(entity, ProtocolKind::Color) => {
                if let Ok(color) = color_query.get(*entity) {
                    info!("add color to entity");

                    let color = {
                        match *color.value {
                            ColorValue::Red => BevyColor::RED,
                            ColorValue::Blue => BevyColor::BLUE,
                            ColorValue::Yellow => BevyColor::YELLOW,
                        }
                    };

                    local.entity(*entity).insert_bundle(SpriteBundle {
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
            _ => {}
        }
    }
}


