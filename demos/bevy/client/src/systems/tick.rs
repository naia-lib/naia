use bevy::{
    ecs::{
        entity::Entity,
        query::With,
        system::{Query, ResMut},
    },
    log::info,
};

use naia_bevy_client::{components::Predicted, Client};

use naia_bevy_demo_shared::protocol::Protocol;

use crate::resources::Global;

pub fn tick(
    mut client: Client<Protocol>,
    mut global: ResMut<Global>,
    q_player_position: Query<Entity, With<Predicted>>,
) {
    //All game logic should happen here, on a tick event
    info!("tick");

    if let Ok(entity) = q_player_position.get_single() {
        if let Some(command) = global.queued_command.take() {
            client.send_command(&entity, command);
        }
    }
}
