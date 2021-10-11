use bevy::{
    ecs::{
        entity::Entity as BevyEntity,
        query::With,
        system::{Commands, Query, ResMut},
    },
    log::info,
    prelude::*,
};

use naia_bevy_client::{Client, Event, Ref, components::{Confirmed, Predicted}};

use naia_bevy_demo_shared::{
    behavior as shared_behavior,
    protocol::{ColorValue, Position, Protocol},
};

use crate::{
    resources::Global,
};

const SQUARE_SIZE: f32 = 32.0;

pub fn receive_events(
    mut local: Commands,
    mut client: Client<Protocol>,
    global: ResMut<Global>,
    mut q_player_position: Query<(BevyEntity, &Ref<Position>), With<Predicted>>,
) {
    for event in client.receive() {
        match event {
            Ok(Event::Connection) => {
                info!("Client connected to: {}", client.server_address());
            }
            Ok(Event::Disconnection) => {
                info!("Client disconnected from: {}", client.server_address());
            }
            Ok(Event::SpawnEntity(entity, component_list)) => {
                local.entity(*entity).insert(Confirmed);

                info!("create entity");

                for component_protocol in component_list {
                    if let Protocol::Color(color_ref) = component_protocol {
                        info!("add color to entity");
                        let color = color_ref.borrow();

                        let material = {
                            match &color.value.get() {
                                ColorValue::Red => global.materials.red.clone(),
                                ColorValue::Blue => global.materials.blue.clone(),
                                ColorValue::Yellow => global.materials.yellow.clone(),
                            }
                        };

                        local.entity(*entity).insert_bundle(SpriteBundle {
                            material: material.clone(),
                            sprite: Sprite::new(Vec2::new(SQUARE_SIZE, SQUARE_SIZE)),
                            transform: Transform::from_xyz(0.0, 0.0, 0.0),
                            ..Default::default()
                        });
                    }
                }
            }
            Ok(Event::OwnEntity(entity)) => {
                info!("gave ownership of entity");

                local
                    .entity(*entity)
                    .insert(Predicted)
                    .insert_bundle(SpriteBundle {
                        material: global.materials.white.clone(),
                        sprite: Sprite::new(Vec2::new(SQUARE_SIZE, SQUARE_SIZE)),
                        transform: Transform::from_xyz(0.0, 0.0, 0.0),
                        ..Default::default()
                    });
            }
            Ok(Event::DisownEntity(_entity)) => {
                info!("removed ownership of entity");
            }
            Ok(Event::NewCommand(entity, Protocol::KeyCommand(key_command_ref)))
            | Ok(Event::ReplayCommand(entity, Protocol::KeyCommand(key_command_ref))) => {
                if let Ok((_, position)) = q_player_position.get_mut(*entity) {
                    shared_behavior::process_command(&key_command_ref, position);
                }
            }
            _ => {}
        }
    }
}
