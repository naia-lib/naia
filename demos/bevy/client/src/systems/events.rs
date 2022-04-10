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
    events::InsertComponentEvent,
    shared::DefaultChannels,
    Client,
};
use naia_bevy_client::events::MessageEvent;
use naia_bevy_demo_shared::Channels;

use naia_bevy_demo_shared::protocol::{Color, ColorValue, Protocol, ProtocolKind};

const SQUARE_SIZE: f32 = 32.0;

pub fn connect_event(client: Client<Protocol, DefaultChannels>) {
    info!("Client connected to: {}", client.server_address());
}

pub fn disconnect_event(client: Client<Protocol, DefaultChannels>) {
    info!("Client disconnected from: {}", client.server_address());
}

pub fn receive_message_event(
    mut event_reader: EventReader<MessageEvent<ProtocolKind, Channels>>,
) {
    for event in event_reader.iter() {
        match event {
            MessageEvent(Channels::EntityAssignment, Protocol::EntityAssignment(message)) => {
                todo!()
            }
            MessageEvent(Channels::PlayerCommand, Protocol::KeyCommand(command)) => {
                todo!()
            }
            _ => {}
        }
    }
}

pub fn insert_component_event(
    mut local: Commands,
    mut event_reader: EventReader<InsertComponentEvent<ProtocolKind>>,
    q_color: Query<&Color>,
) {
    for InsertComponentEvent(entity, component_kind) in event_reader.iter() {
        match component_kind {
            ProtocolKind::Color => {
                if let Ok(color) = q_color.get(*entity) {
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
