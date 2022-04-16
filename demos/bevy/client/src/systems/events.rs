use bevy::{
    ecs::{
        event::EventReader,
        system::{Commands, Query},
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

const SQUARE_SIZE: f32 = 32.0;

pub fn connect_event(client: Client<Protocol, Channels>) {
    info!("Client connected to: {}", client.server_address());
}

pub fn disconnect_event(client: Client<Protocol, Channels>) {
    info!("Client disconnected from: {}", client.server_address());
}

pub fn receive_message_event(mut event_reader: EventReader<MessageEvent<Protocol, Channels>>) {
    for event in event_reader.iter() {
        match event {
            MessageEvent(Channels::EntityAssignment, Protocol::EntityAssignment(_message)) => {
                todo!()
            }
            MessageEvent(Channels::PlayerCommand, Protocol::KeyCommand(_command)) => {
                todo!()
            }
            _ => {
                unimplemented!()
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

pub fn spawn_entity_event(mut _local: Commands, mut event_reader: EventReader<SpawnEntityEvent>) {
    for event in event_reader.iter() {
        match event {
            SpawnEntityEvent(entity) => {
                info!("spawned!");
            }
        }
    }
}
