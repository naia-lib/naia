use bevy::{ecs::{entity::Entity as BevyEntity, system::{Query, ResMut}, query::With}, log::info};

use naia_bevy_client::{Client, Ref, components::Predicted, Entity};

use naia_bevy_demo_shared::protocol::{Protocol, Position};

use crate::resources::Global;

pub fn tick(
    mut client: Client<Protocol>,
    mut global: ResMut<Global>,
    q_player_position: Query<(BevyEntity, &Ref<Position>), With<Predicted>>) {
    // All game logic should happen here, on a tick event
    info!("tick");

    for (entity, _) in q_player_position.iter() {
        if let Some(command) = global.queued_command.take() {
            client.queue_command(&Entity::new(entity), &command);
        }
    }
}
