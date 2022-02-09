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
    events::SpawnEntityEvent,
    Client,
};
use naia_bevy_demo_shared::{
    protocol::{Color, ColorValue, Protocol, ProtocolKind},
};

const SQUARE_SIZE: f32 = 32.0;

pub fn connect_event(client: Client<Protocol>) {
    info!("Client connected to: {}", client.server_address());
}

pub fn disconnect_event(client: Client<Protocol>) {
    info!("Client disconnected from: {}", client.server_address());
}

pub fn spawn_entity_event(
    mut local: Commands,
    mut event_reader: EventReader<SpawnEntityEvent<Protocol>>,
    q_color: Query<&Color>,
) {
    for SpawnEntityEvent(entity, component_kinds) in event_reader.iter() {
        info!("create entity");

        for component_kind in component_kinds {
            match component_kind {
                ProtocolKind::Color => {
                    if let Ok(color) = q_color.get(*entity) {
                        info!("add color to entity");

                        let color = {
                            match &color.value.get() {
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
}
