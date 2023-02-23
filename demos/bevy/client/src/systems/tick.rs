use bevy_ecs::{
    event::EventReader,
    system::{Query, ResMut},
};
use bevy_log::info;

use naia_bevy_client::{events::ClientTickEvent, Client};
use naia_bevy_demo_shared::{
    behavior as shared_behavior, channels::PlayerCommandChannel, components::Position,
    messages::KeyCommand,
};

use crate::resources::Global;

pub fn tick_events(
    mut tick_reader: EventReader<ClientTickEvent>,
    mut global: ResMut<Global>,
    mut client: Client,
    mut position_query: Query<&mut Position>,
) {
    if !client.is_connected() {
        return;
    }
    let Some(command) = global.queued_command.take() else {
        info!("Command aborted: Queued Command empty");
        return;
    };

    let Some(predicted_entity) = global
        .owned_entity
        .as_ref()
        .map(|owned_entity| owned_entity.predicted) else {
        info!("Command aborted: no Owned Entity");
        return;
    };

    for ClientTickEvent(tick) in tick_reader.iter() {
        //info!("--- Tick: {tick}");

        //
        let last_client_tick = global.last_client_tick;
        if *tick != last_client_tick.wrapping_add(1) {
            info!("Skipped? Last Tick at: {last_client_tick}, Current Command at {tick}");
        }
        global.last_client_tick = *tick;
        //

        //All game logic should happen here, on a tick event
        if !global.command_history.can_insert(tick) {
            info!("Command aborted: History full");
            continue;
        }

        // Record command
        global.command_history.insert(*tick, command.clone());

        // Send command
        client.send_tick_buffer_message::<PlayerCommandChannel, KeyCommand>(tick, &command);

        // Apply command
        if let Ok(mut position) = position_query.get_mut(predicted_entity) {
            shared_behavior::process_command(&command, &mut position);
        }
    }
}
