use std::ops::DerefMut;

use bevy_ecs::{
    event::Events,
    world::{Mut, World},
};

use log::warn;

use naia_bevy_shared::{HostOwned, HostSyncEvent, WorldMutType, WorldProxy, WorldProxyMut};
use naia_server::EntityOwner;

use crate::{ClientOwned, EntityAuthStatus, server::ServerWrapper};

mod naia_events {
    pub use naia_server::{
        ConnectEvent, DelegateEntityEvent, DespawnEntityEvent, DisconnectEvent,
        EntityAuthGrantEvent, EntityAuthResetEvent, ErrorEvent, InsertComponentEvent,
        PublishEntityEvent, RemoveComponentEvent, SpawnEntityEvent, TickEvent,
        UnpublishEntityEvent, UpdateComponentEvent,
    };
}

mod bevy_events {
    pub use crate::events::{
        AuthEvents, ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent,
        InsertComponentEvents, MessageEvents, PublishEntityEvent, RemoveComponentEvents,
        SpawnEntityEvent, TickEvent, UnpublishEntityEvent, UpdateComponentEvents,
    };
}

pub fn before_receive_events(world: &mut World) {
    world.resource_scope(|world, mut server: Mut<ServerWrapper>| {
        if !server.0.is_listening() {
            return;
        }

        // Host Component Updates
        let mut host_component_event_reader = world
            .get_resource_mut::<Events<HostSyncEvent>>()
            .unwrap();
        let host_component_events: Vec<HostSyncEvent> = host_component_event_reader.drain().collect();
        for event in host_component_events {
            match event {
                HostSyncEvent::Insert(entity, component_kind) => {
                    if server.0.entity_authority_status(&entity) == Some(EntityAuthStatus::Denied) {
                        // if auth status is denied, that means the client is performing this operation and it's already being handled
                        continue;
                    }
                    let mut world_proxy = world.proxy_mut();
                    let Some(mut component_mut) = world_proxy.component_mut_of_kind(&entity, &component_kind) else {
                        warn!("could not find Component in World which has just been inserted!");
                        continue;
                    };
                    server.0.insert_component_worldless(&entity, DerefMut::deref_mut(&mut component_mut));
                }
                HostSyncEvent::Remove(entity, component_kind) => {
                    if server.0.entity_authority_status(&entity) == Some(EntityAuthStatus::Denied) {
                        // if auth status is denied, that means the client is performing this operation and it's already being handled
                        continue;
                    }
                    server.0.remove_component_worldless(&entity, &component_kind);
                }
                HostSyncEvent::Despawn(entity) => {
                    if server.0.entity_authority_status(&entity) == Some(EntityAuthStatus::Denied) {
                        // if auth status is denied, that means the client is performing this operation and it's already being handled
                        continue;
                    }
                    server.0.despawn_entity_worldless(&entity);
                }
            }
        }

        // Receive Events
        let mut did_tick = false;
        let mut events = server.0.receive(world.proxy_mut());
        if !events.is_empty() {

            // Connect Event
            if events.has::<naia_events::ConnectEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::ConnectEvent>>()
                    .unwrap();
                for user_key in events.read::<naia_events::ConnectEvent>() {
                    event_writer.send(bevy_events::ConnectEvent(user_key));
                }
            }

            // Disconnect Event
            if events.has::<naia_events::DisconnectEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::DisconnectEvent>>()
                    .unwrap();
                for (user_key, user) in events.read::<naia_events::DisconnectEvent>() {
                    event_writer.send(bevy_events::DisconnectEvent(user_key, user));
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

            // Tick Event
            if events.has::<naia_events::TickEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::TickEvent>>()
                    .unwrap();
                for tick in events.read::<naia_events::TickEvent>() {
                    event_writer.send(bevy_events::TickEvent(tick));
                    did_tick = true;
                }
            }

            // Message Event
            if events.has_messages() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::MessageEvents>>()
                    .unwrap();
                event_writer.send(bevy_events::MessageEvents::from(&mut events));
            }

            // Auth Event
            if events.has_auths() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::AuthEvents>>()
                    .unwrap();
                event_writer.send(bevy_events::AuthEvents::from(&mut events));
            }

            // Spawn Entity Event
            if events.has::<naia_events::SpawnEntityEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::SpawnEntityEvent>>()
                    .unwrap();
                let mut spawned_entities = Vec::new();
                for (user_key, entity) in events.read::<naia_events::SpawnEntityEvent>() {
                    spawned_entities.push(entity);
                    event_writer.send(bevy_events::SpawnEntityEvent(user_key, entity));
                }
                for entity in spawned_entities {
                    let EntityOwner::Client(user_key) = server.0.entity_owner(&entity) else {
                        panic!("spawned entity that doesn't belong to a client ... shouldn't be possible.");
                    };
                    world.entity_mut(entity).insert(ClientOwned(user_key));
                }
            }

            // Despawn Entity Event
            if events.has::<naia_events::DespawnEntityEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::DespawnEntityEvent>>()
                    .unwrap();
                for (user_key, entity) in events.read::<naia_events::DespawnEntityEvent>() {
                    event_writer.send(bevy_events::DespawnEntityEvent(user_key, entity));
                }
            }

            // Publish Entity Event
            if events.has::<naia_events::PublishEntityEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::PublishEntityEvent>>()
                    .unwrap();
                for (user_key, entity) in events.read::<naia_events::PublishEntityEvent>() {
                    event_writer.send(bevy_events::PublishEntityEvent(user_key, entity));
                }
            }

            // Unpublish Entity Event
            if events.has::<naia_events::UnpublishEntityEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::UnpublishEntityEvent>>()
                    .unwrap();
                for (user_key, entity) in events.read::<naia_events::UnpublishEntityEvent>() {
                    event_writer.send(bevy_events::UnpublishEntityEvent(user_key, entity));
                }
            }

            // Delegate Entity Event
            if events.has::<naia_events::DelegateEntityEvent>() {
                for (_, entity) in events.read::<naia_events::DelegateEntityEvent>() {
                    world.entity_mut(entity).insert(HostOwned);
                }
            }

            // Entity Auth Given Event
            if events.has::<naia_events::EntityAuthGrantEvent>() {
                for (_, entity) in events.read::<naia_events::EntityAuthGrantEvent>() {
                    world.entity_mut(entity).remove::<HostOwned>();
                }
            }

            // Entity Auth Reset Event
            if events.has::<naia_events::EntityAuthResetEvent>() {
                for entity in events.read::<naia_events::EntityAuthResetEvent>() {
                    if let Some(mut entity_mut) = world.get_entity_mut(entity) {
                        entity_mut.insert(HostOwned);
                    }
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

            if did_tick {
                server.0.send_all_updates(world.proxy());
            }
        }
    });
}
