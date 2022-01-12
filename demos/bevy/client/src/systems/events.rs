use bevy::{
    ecs::{
        query::With,
        system::{Commands, Query, ResMut},
    },
    log::info,
    prelude::*,
};

use naia_bevy_client::{
    components::Predicted,
    events::{NewCommandEvent, OwnEntityEvent, ReplayCommandEvent, SpawnEntityEvent},
    Client,
};

use naia_bevy_demo_shared::{
    behavior as shared_behavior,
    protocol::{Color, ColorValue, Position, Protocol, ProtocolKind},
};

use crate::resources::Global;

const SQUARE_SIZE: f32 = 32.0;

pub fn connect_event(client: Client<Protocol>) {
    info!("Client connected to: {}", client.server_address());
}

pub fn disconnect_event(client: Client<Protocol>) {
    info!("Client disconnected from: {}", client.server_address());
}

pub fn spawn_entity_event(
    mut local: Commands,
    global: ResMut<Global>,
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

                        let material = {
                            match &color.value.get() {
                                ColorValue::Red => global.materials.red.clone(),
                                ColorValue::Blue => global.materials.blue.clone(),
                                ColorValue::Yellow => global.materials.yellow.clone(),
                            }
                        };

                        // local.entity(*entity).insert_bundle(SpriteBundle {
                        //     material: material.clone(),
                        //     sprite: Sprite::new(Vec2::new(SQUARE_SIZE, SQUARE_SIZE)),
                        //     transform: Transform::from_xyz(0.0, 0.0, 0.0),
                        //     ..Default::default()
                        // });
                    }
                }
                _ => {}
            }
        }
    }
}

pub fn own_entity_event(
    mut local: Commands,
    global: ResMut<Global>,
    mut event_reader: EventReader<OwnEntityEvent>,
) {
    for OwnEntityEvent(owned_entity) in event_reader.iter() {
        info!("gave ownership of entity");

        let predicted_entity = owned_entity.predicted;
        //
        // local.entity(predicted_entity).insert_bundle(SpriteBundle {
        //     material: global.materials.white.clone(),
        //     sprite: Sprite::new(Vec2::new(SQUARE_SIZE, SQUARE_SIZE)),
        //     transform: Transform::from_xyz(0.0, 0.0, 0.0),
        //     ..Default::default()
        // });
    }
}

pub fn new_command_event(
    mut event_reader: EventReader<NewCommandEvent<Protocol>>,
    mut q_player_position: Query<&mut Position, With<Predicted>>,
) {
    for event in event_reader.iter() {
        if let NewCommandEvent(owned_entity, Protocol::KeyCommand(command)) = event {
            let predicted_entity = owned_entity.predicted;
            if let Ok(mut position) = q_player_position.get_mut(predicted_entity) {
                // shared_behavior::process_command(command, &mut position);
            }
        }
    }
}

pub fn replay_command_event(
    mut event_reader: EventReader<ReplayCommandEvent<Protocol>>,
    mut q_player_position: Query<&mut Position, With<Predicted>>,
) {
    for event in event_reader.iter() {
        if let ReplayCommandEvent(owned_entity, Protocol::KeyCommand(command)) = event {
            let predicted_entity = owned_entity.predicted;
            if let Ok(mut position) = q_player_position.get_mut(predicted_entity) {
                // shared_behavior::process_command(command, &mut position);
            }
        }
    }
}
