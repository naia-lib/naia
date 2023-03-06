use std::ops::DerefMut;

use bevy_ecs::{
    entity::Entity,
    event::Events,
    world::{Mut, World},
};

use naia_bevy_shared::{HostComponentEvent, WorldMutType, WorldProxyMut};
use naia_client::Client;

use crate::ServerOwned;

mod naia_events {
    pub use naia_client::{
        ClientTickEvent, ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent,
        RejectEvent, ServerTickEvent, SpawnEntityEvent,
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

        // Host Component Updates
        let mut host_component_event_reader = world
            .get_resource_mut::<Events<HostComponentEvent>>()
            .unwrap();
        let host_component_events: Vec<HostComponentEvent> = host_component_event_reader.drain().collect();
        for HostComponentEvent(added, entity, component_kind) in host_component_events {
            if added {
                let mut world_proxy = world.proxy_mut();
                let Some(mut component_mut) = world_proxy.component_mut_of_kind(&entity, &component_kind) else {
                    continue;
                };
                client.insert_component_worldless(&entity, DerefMut::deref_mut(&mut component_mut));
            } else {
                client.remove_component_worldless(&entity, &component_kind);
            }
        }

        // Receive Events
        let mut events = client.receive(world.proxy_mut());
        if !events.is_empty() {

            if events.has::<naia_events::ConnectEvent>() {
                // Connect Event
                let mut connect_event_writer = world
                    .get_resource_mut::<Events<bevy_events::ConnectEvent>>()
                    .unwrap();
                for _ in events.read::<naia_events::ConnectEvent>() {
                    connect_event_writer.send(bevy_events::ConnectEvent);
                }
            }

            // Disconnect Event
            if events.has::<naia_events::DisconnectEvent>() {
                let mut disconnect_event_writer = world
                    .get_resource_mut::<Events<bevy_events::DisconnectEvent>>()
                    .unwrap();
                for _ in events.read::<naia_events::DisconnectEvent>() {
                    disconnect_event_writer.send(bevy_events::DisconnectEvent);
                }
            }

            // Reject Event
            if events.has::<naia_events::RejectEvent>() {
                let mut reject_event_writer = world
                    .get_resource_mut::<Events<bevy_events::RejectEvent>>()
                    .unwrap();
                for _ in events.read::<naia_events::RejectEvent>() {
                    reject_event_writer.send(bevy_events::RejectEvent);
                }
            }

            // Error Event
            if events.has::<naia_events::ErrorEvent>() {
                let mut error_event_writer = world
                    .get_resource_mut::<Events<bevy_events::ErrorEvent>>()
                    .unwrap();
                for error in events.read::<naia_events::ErrorEvent>() {
                    error_event_writer.send(bevy_events::ErrorEvent(error));
                }
            }

            // Client Tick Event
            if events.has::<naia_events::ClientTickEvent>() {
                let mut client_tick_event_writer = world
                    .get_resource_mut::<Events<bevy_events::ClientTickEvent>>()
                    .unwrap();
                for tick in events.read::<naia_events::ClientTickEvent>() {
                    client_tick_event_writer.send(bevy_events::ClientTickEvent(tick));
                }
            }

            // Server Tick Event
            if events.has::<naia_events::ServerTickEvent>() {
                let mut server_tick_event_writer = world
                    .get_resource_mut::<Events<bevy_events::ServerTickEvent>>()
                    .unwrap();
                for tick in events.read::<naia_events::ServerTickEvent>() {
                    server_tick_event_writer.send(bevy_events::ServerTickEvent(tick));
                }
            }

            // Message Event
            if events.has_messages() {
                let mut message_event_writer = world
                    .get_resource_mut::<Events<bevy_events::MessageEvents>>()
                    .unwrap();
                message_event_writer.send(bevy_events::MessageEvents::from(&mut events));
            }

            // Spawn Entity Event
            if events.has::<naia_events::SpawnEntityEvent>() {
                let mut spawn_entity_event_writer = world
                    .get_resource_mut::<Events<bevy_events::SpawnEntityEvent>>()
                    .unwrap();

                let mut spawned_entities = Vec::new();
                for entity in events.read::<naia_events::SpawnEntityEvent>() {
                    spawned_entities.push(entity);
                    spawn_entity_event_writer.send(bevy_events::SpawnEntityEvent(entity));
                }
                for entity in spawned_entities {
                    world.entity_mut(entity).insert(ServerOwned);
                }
            }

            // Despawn Entity Event
            if events.world.has::<naia_events::DespawnEntityEvent>() {
                let mut despawn_entity_event_writer = world
                    .get_resource_mut::<Events<bevy_events::DespawnEntityEvent>>()
                    .unwrap();
                for entity in events.world.read::<naia_events::DespawnEntityEvent>() {
                    despawn_entity_event_writer.send(bevy_events::DespawnEntityEvent(entity));
                }
            }

            // Insert Component Event
            if events.world.has_inserts() {
                let inserts = events.world.take_inserts().unwrap();
                let mut insert_component_event_writer = world
                    .get_resource_mut::<Events<bevy_events::InsertComponentEvents>>()
                    .unwrap();
                insert_component_event_writer.send(bevy_events::InsertComponentEvents::new(inserts));
            }

            // Update Component Event
            if events.world.has_updates() {
                let updates = events.world.take_updates().unwrap();
                let mut update_component_event_writer = world
                    .get_resource_mut::<Events<bevy_events::UpdateComponentEvents>>()
                    .unwrap();
                update_component_event_writer
                    .send(bevy_events::UpdateComponentEvents::new(updates));
            }

            // Remove Component Event
            if events.world.has_removes() {
                let removes = events.world.take_removes().unwrap();
                let mut remove_component_event_writer = world
                    .get_resource_mut::<Events<bevy_events::RemoveComponentEvents>>()
                    .unwrap();

                remove_component_event_writer.send(bevy_events::RemoveComponentEvents::new(removes));
            }
        }
    });
}
