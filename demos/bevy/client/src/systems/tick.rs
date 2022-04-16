use bevy::ecs::{
    entity::Entity,
    query::With,
    system::{Query, ResMut},
};

use naia_bevy_client::{components::Predicted, Client};

use naia_bevy_demo_shared::{protocol::Protocol, Channels};

use crate::resources::Global;

pub fn tick(
    mut _client: Client<Protocol, Channels>,
    mut global: ResMut<Global>,
    q_player_position: Query<Entity, With<Predicted>>,
) {
    //All game logic should happen here, on a tick event
    //info!("tick");

    if let Ok(_entity) = q_player_position.get_single() {
        if let Some(_command) = global.queued_command.take() {
            //client.send_command(&entity, command);
            unimplemented!();
        }
    }
}
