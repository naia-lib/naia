use std::ops::DerefMut;

use bevy_ecs::{
    entity::Entity,
    event::Events,
    world::{Mut, World},
};

use naia_bevy_shared::{HostSyncEvent, WorldMutType, WorldProxyMut};
use naia_client::Client;

mod naia_events {
    pub use naia_client::{
        ClientTickEvent, ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent,
        PublishEntityEvent, RejectEvent, ServerTickEvent, SpawnEntityEvent, UnpublishEntityEvent,
        EntityAuthGrantedEvent, EntityAuthDeniedEvent, EntityAuthResetEvent,
    };
}

mod bevy_events {
    pub use crate::events::{
        ClientTickEvent, ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent,
        InsertComponentEvents, MessageEvents, PublishEntityEvent, RejectEvent,
        RemoveComponentEvents, ServerTickEvent, SpawnEntityEvent, UnpublishEntityEvent,
        UpdateComponentEvents, EntityAuthGrantedEvent, EntityAuthDeniedEvent, EntityAuthResetEvent,
    };
}

use crate::ServerOwned;

pub fn before_receive_events(world: &mut World) {
    world.resource_scope(|world, mut client: Mut<Client<Entity>>| {

        // Host Component Updates
        let mut host_component_event_reader = world
            .get_resource_mut::<Events<HostSyncEvent>>()
            .unwrap();
        let host_component_events: Vec<HostSyncEvent> = host_component_event_reader.drain().collect();
        for event in host_component_events {
            match event {
                HostSyncEvent::Insert(entity, component_kind) => {
                    let mut world_proxy = world.proxy_mut();
                    let Some(mut component_mut) = world_proxy.component_mut_of_kind(&entity, &component_kind) else {
                        continue;
                    };
                    client.insert_component_worldless(&entity, DerefMut::deref_mut(&mut component_mut));
                }
                HostSyncEvent::Remove(entity, component_kind) => {
                    client.remove_component_worldless(&entity, &component_kind);
                }
                HostSyncEvent::Despawn(entity) => {
                    client.despawn_entity_worldless(&entity);
                }
            }
        }

        // Receive Events
        let mut events = client.receive(world.proxy_mut());
        if !events.is_empty() {

            if events.has::<naia_events::ConnectEvent>() {
                // Connect Event
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::ConnectEvent>>()
                    .unwrap();
                for _ in events.read::<naia_events::ConnectEvent>() {
                    event_writer.send(bevy_events::ConnectEvent);
                }
            }

            // Disconnect Event
            if events.has::<naia_events::DisconnectEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::DisconnectEvent>>()
                    .unwrap();
                for _ in events.read::<naia_events::DisconnectEvent>() {
                    event_writer.send(bevy_events::DisconnectEvent);
                }
            }

            // Reject Event
            if events.has::<naia_events::RejectEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::RejectEvent>>()
                    .unwrap();
                for _ in events.read::<naia_events::RejectEvent>() {
                    event_writer.send(bevy_events::RejectEvent);
                }
            }

            // Error Event
            if events.has::<naia_events::ErrorEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::ErrorEvent>>()
                    .unwrap();
                for error in events.read::<naia_events::ErrorEvent>() {
                    event_writer.send(bevy_events::ErrorEvent(error));
                }
            }

            // Client Tick Event
            if events.has::<naia_events::ClientTickEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::ClientTickEvent>>()
                    .unwrap();
                for tick in events.read::<naia_events::ClientTickEvent>() {
                    event_writer.send(bevy_events::ClientTickEvent(tick));
                }
            }

            // Server Tick Event
            if events.has::<naia_events::ServerTickEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::ServerTickEvent>>()
                    .unwrap();
                for tick in events.read::<naia_events::ServerTickEvent>() {
                    event_writer.send(bevy_events::ServerTickEvent(tick));
                }
            }

            // Message Event
            if events.has_messages() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::MessageEvents>>()
                    .unwrap();
                event_writer.send(bevy_events::MessageEvents::from(&mut events));
            }

            // Spawn Entity Event
            if events.has::<naia_events::SpawnEntityEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::SpawnEntityEvent>>()
                    .unwrap();

                let mut spawned_entities = Vec::new();
                for entity in events.read::<naia_events::SpawnEntityEvent>() {
                    spawned_entities.push(entity);
                    event_writer.send(bevy_events::SpawnEntityEvent(entity));
                }
                for entity in spawned_entities {
                    world.entity_mut(entity).insert(ServerOwned);
                }
            }

            // Despawn Entity Event
            if events.has::<naia_events::DespawnEntityEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::DespawnEntityEvent>>()
                    .unwrap();
                for entity in events.read::<naia_events::DespawnEntityEvent>() {
                    event_writer.send(bevy_events::DespawnEntityEvent(entity));
                }
            }

            // Publish Entity Event
            if events.has::<naia_events::PublishEntityEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::PublishEntityEvent>>()
                    .unwrap();
                for entity in events.read::<naia_events::PublishEntityEvent>() {
                    event_writer.send(bevy_events::PublishEntityEvent(entity));
                }
            }

            // Unpublish Entity Event
            if events.has::<naia_events::UnpublishEntityEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::UnpublishEntityEvent>>()
                    .unwrap();
                for entity in events.read::<naia_events::UnpublishEntityEvent>() {
                    event_writer.send(bevy_events::UnpublishEntityEvent(entity));
                }
            }

            // Entity Auth Granted Event
            if events.has::<naia_events::EntityAuthGrantedEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::EntityAuthGrantedEvent>>()
                    .unwrap();
                for entity in events.read::<naia_events::EntityAuthGrantedEvent>() {
                    event_writer.send(bevy_events::EntityAuthGrantedEvent(entity));
                }
            }

            // Entity Auth Denied Event
            if events.has::<naia_events::EntityAuthDeniedEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::EntityAuthDeniedEvent>>()
                    .unwrap();
                for entity in events.read::<naia_events::EntityAuthDeniedEvent>() {
                    event_writer.send(bevy_events::EntityAuthDeniedEvent(entity));
                }
            }

            // Entity Auth Reset Event
            if events.has::<naia_events::EntityAuthResetEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::EntityAuthResetEvent>>()
                    .unwrap();
                for entity in events.read::<naia_events::EntityAuthResetEvent>() {
                    event_writer.send(bevy_events::EntityAuthResetEvent(entity));
                }
            }

            // Insert Component Event
            if events.has_inserts() {
                let inserts = events.take_inserts().unwrap();
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::InsertComponentEvents>>()
                    .unwrap();
                event_writer.send(bevy_events::InsertComponentEvents::new(inserts));
            }

            // Update Component Event
            if events.has_updates() {
                let updates = events.take_updates().unwrap();
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::UpdateComponentEvents>>()
                    .unwrap();
                event_writer
                    .send(bevy_events::UpdateComponentEvents::new(updates));
            }

            // Remove Component Event
            if events.has_removes() {
                let removes = events.take_removes().unwrap();
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::RemoveComponentEvents>>()
                    .unwrap();

                event_writer.send(bevy_events::RemoveComponentEvents::new(removes));
            }
        }
    });
}
