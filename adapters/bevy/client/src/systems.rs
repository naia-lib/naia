use bevy_ecs::event::Events;
use bevy_ecs::{
    entity::Entity,
    schedule::ShouldRun,
    system::Res,
    world::{Mut, World},
};

use naia_client::Client;

use naia_bevy_shared::WorldProxyMut;

mod naia_events {
    pub use naia_client::{
        ClientTickEvent, ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent,
        InsertComponentEvent, MessageEvent, RejectEvent, RemoveComponentEvent, ServerTickEvent,
        SpawnEntityEvent, UpdateComponentEvent,
    };
}

mod bevy_events {
    pub use crate::events::{
        ClientTickEvent, ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent,
        InsertComponentEvents, MessageEvents, RejectEvent, RemoveComponentEvents, ServerTickEvent,
        SpawnEntityEvent, UpdateComponentEvents,
    };
}

pub fn before_receive_events(world: &mut World) {
    world.resource_scope(|world, mut client: Mut<Client<Entity>>| {
        let mut events = client.receive(world.proxy_mut());
        if !events.is_empty() {
            unsafe {
                // Connect Event
                let mut connect_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::ConnectEvent>>()
                    .unwrap();
                for _ in events.read::<naia_events::ConnectEvent>() {
                    connect_event_writer.send(bevy_events::ConnectEvent);
                }

                // Disconnect Event
                let mut disconnect_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::DisconnectEvent>>()
                    .unwrap();
                for _ in events.read::<naia_events::DisconnectEvent>() {
                    disconnect_event_writer.send(bevy_events::DisconnectEvent);
                }

                // Reject Event
                let mut reject_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::RejectEvent>>()
                    .unwrap();
                for _ in events.read::<naia_events::RejectEvent>() {
                    reject_event_writer.send(bevy_events::RejectEvent);
                }

                // Error Event
                let mut error_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::ErrorEvent>>()
                    .unwrap();
                for error in events.read::<naia_events::ErrorEvent>() {
                    error_event_writer.send(bevy_events::ErrorEvent(error));
                }

                // Client Tick Event
                let mut client_tick_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::ClientTickEvent>>()
                    .unwrap();
                for tick in events.read::<naia_events::ClientTickEvent>() {
                    client_tick_event_writer.send(bevy_events::ClientTickEvent(tick));
                }

                // Server Tick Event
                let mut server_tick_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::ServerTickEvent>>()
                    .unwrap();
                for tick in events.read::<naia_events::ServerTickEvent>() {
                    server_tick_event_writer.send(bevy_events::ServerTickEvent(tick));
                }

                // Message Event
                let mut message_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::MessageEvents>>()
                    .unwrap();
                message_event_writer.send(bevy_events::MessageEvents::from(&mut events));

                // Spawn Entity Event
                let mut spawn_entity_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::SpawnEntityEvent>>()
                    .unwrap();
                for entity in events.read::<naia_events::SpawnEntityEvent>() {
                    spawn_entity_event_writer.send(bevy_events::SpawnEntityEvent(entity));
                }

                // Despawn Entity Event
                let mut despawn_entity_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::DespawnEntityEvent>>()
                    .unwrap();
                for entity in events.read::<naia_events::DespawnEntityEvent>() {
                    despawn_entity_event_writer.send(bevy_events::DespawnEntityEvent(entity));
                }

                // Insert Component Event
                let mut insert_component_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::InsertComponentEvents>>()
                    .unwrap();
                if events.has_inserts() {
                    insert_component_event_writer.send(bevy_events::InsertComponentEvents::new(
                        events.take_inserts(),
                    ));
                }

                // Update Component Event
                let mut update_component_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::UpdateComponentEvents>>()
                    .unwrap();
                if events.has_updates() {
                    update_component_event_writer.send(bevy_events::UpdateComponentEvents::new(
                        events.take_updates(),
                    ));
                }

                // Remove Component Event
                let mut remove_component_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::RemoveComponentEvents>>()
                    .unwrap();
                if events.has_removes() {
                    remove_component_event_writer.send(bevy_events::RemoveComponentEvents::new(
                        events.take_removes(),
                    ));
                }
            }
        }
    });
}

pub fn should_receive(client: Res<Client<Entity>>) -> ShouldRun {
    if client.is_connecting() {
        ShouldRun::Yes
    } else {
        ShouldRun::No
    }
}
