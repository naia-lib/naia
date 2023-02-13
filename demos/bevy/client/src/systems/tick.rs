use bevy::ecs::system::{Query, ResMut};

use naia_bevy_client::Client;

use naia_bevy_demo_shared::channels::PlayerCommandChannel;
use naia_bevy_demo_shared::messages::KeyCommand;
use naia_bevy_demo_shared::{behavior as shared_behavior, components::Position};

use crate::resources::Global;

pub fn tick(
    mut global: ResMut<Global>,
    mut client: Client,
    mut position_query: Query<&mut Position>,
) {
    //All game logic should happen here, on a tick event

    if let Some(command) = global.queued_command.take() {
        if let Some(predicted_entity) = global
            .owned_entity
            .as_ref()
            .map(|owned_entity| owned_entity.predicted)
        {
            if let Some(client_tick) = client.client_tick() {
                if global.command_history.can_insert(&client_tick) {
                    // Record command
                    global.command_history.insert(client_tick, command.clone());

                    // Send command
                    client.send_message::<PlayerCommandChannel, KeyCommand>(&command);

                    // Apply command
                    if let Ok(mut position) = position_query.get_mut(predicted_entity) {
                        shared_behavior::process_command(&command, &mut position);
                    }
                }
            }
        }
    }
}
