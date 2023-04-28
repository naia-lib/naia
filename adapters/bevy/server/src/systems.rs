use std::ops::DerefMut;

use bevy_ecs::{
    entity::Entity,
    event::Events,
    world::{Mut, World},
};

use naia_bevy_shared::{HostSyncEvent, WorldMutType, WorldProxy, WorldProxyMut};
use naia_server::{EntityOwner, Server};

use crate::ClientOwned;

mod naia_events {
    pub use naia_server::{
        ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent, InsertComponentEvent,
        PublishEntityEvent, RemoveComponentEvent, SpawnEntityEvent, TickEvent,
        UpdateComponentEvent,
    };
}

mod bevy_events {
    pub use crate::events::{
        AuthEvents, ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent,
        InsertComponentEvents, MessageEvents, PublishEntityEvent, RemoveComponentEvents,
        SpawnEntityEvent, TickEvent, UpdateComponentEvents,
    };
}

pub fn before_receive_events(world: &mut World) {
    world.resource_scope(|world, mut server: Mut<Server<Entity>>| {
        if !server.is_listening() {
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
                    let mut world_proxy = world.proxy_mut();
                    let Some(mut component_mut) = world_proxy.component_mut_of_kind(&entity, &component_kind) else {
                        continue;
                    };
                    server.insert_component_worldless(&entity, DerefMut::deref_mut(&mut component_mut));
                }
                HostSyncEvent::Remove(entity, component_kind) => {
                    server.remove_component_worldless(&entity, &component_kind);
                }
                HostSyncEvent::Despawn(entity) => {
                    server.despawn_entity_worldless(&entity);
                }
            }
        }

        // Receive Events
        let mut did_tick = false;
        let mut events = server.receive(world.proxy_mut());
        if !events.is_empty() {

            // Connect Event
            if events.has::<naia_events::ConnectEvent>() {
                let mut connect_event_writer = world
                    .get_resource_mut::<Events<bevy_events::ConnectEvent>>()
                    .unwrap();
                for user_key in events.read::<naia_events::ConnectEvent>() {
                    connect_event_writer.send(bevy_events::ConnectEvent(user_key));
                }
            }

            // Disconnect Event
            if events.has::<naia_events::DisconnectEvent>() {
                let mut disconnect_event_writer = world
                    .get_resource_mut::<Events<bevy_events::DisconnectEvent>>()
                    .unwrap();
                for (user_key, user) in events.read::<naia_events::DisconnectEvent>() {
                    disconnect_event_writer.send(bevy_events::DisconnectEvent(user_key, user));
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

            // Tick Event
            if events.has::<naia_events::TickEvent>() {
                let mut tick_event_writer = world
                    .get_resource_mut::<Events<bevy_events::TickEvent>>()
                    .unwrap();
                for tick in events.read::<naia_events::TickEvent>() {
                    tick_event_writer.send(bevy_events::TickEvent(tick));
                    did_tick = true;
                }
            }

            // Message Event
            if events.has_messages() {
                let mut message_event_writer = world
                    .get_resource_mut::<Events<bevy_events::MessageEvents>>()
                    .unwrap();
                message_event_writer.send(bevy_events::MessageEvents::from(&mut events));
            }

            // Auth Event
            if events.has_auths() {
                let mut auth_event_writer = world
                    .get_resource_mut::<Events<bevy_events::AuthEvents>>()
                    .unwrap();
                auth_event_writer.send(bevy_events::AuthEvents::from(&mut events));
            }

            // Spawn Entity Event
            if events.has::<naia_events::SpawnEntityEvent>() {
                let mut spawn_entity_event_writer = world
                    .get_resource_mut::<Events<bevy_events::SpawnEntityEvent>>()
                    .unwrap();
                let mut spawned_entities = Vec::new();
                for (user_key, entity) in events.read::<naia_events::SpawnEntityEvent>() {
                    spawned_entities.push(entity);
                    spawn_entity_event_writer.send(bevy_events::SpawnEntityEvent(user_key, entity));
                }
                for entity in spawned_entities {
                    let EntityOwner::Client(user_key) = server.entity_owner(&entity) else {
                        panic!("spawned entity that doesn't belong to a client ... shouldn't be possible.");
                    };
                    world.entity_mut(entity).insert(ClientOwned(user_key));
                }
            }

            // Despawn Entity Event
            if events.has::<naia_events::DespawnEntityEvent>() {
                let mut despawn_entity_event_writer = world
                    .get_resource_mut::<Events<bevy_events::DespawnEntityEvent>>()
                    .unwrap();
                for (user_key, entity) in events.read::<naia_events::DespawnEntityEvent>() {
                    despawn_entity_event_writer.send(bevy_events::DespawnEntityEvent(user_key, entity));
                }
            }

            // Publish Entity Event
            if events.has::<naia_events::PublishEntityEvent>() {
                let mut publish_entity_event_writer = world
                    .get_resource_mut::<Events<bevy_events::PublishEntityEvent>>()
                    .unwrap();
                for (user_key, entity) in events.read::<naia_events::PublishEntityEvent>() {
                    publish_entity_event_writer.send(bevy_events::PublishEntityEvent(user_key, entity));
                }
            }

            // Insert Component Event
            if events.has_inserts() {
                let inserts = events.take_inserts().unwrap();
                let mut insert_component_event_writer = world
                    .get_resource_mut::<Events<bevy_events::InsertComponentEvents>>()
                    .unwrap();
                insert_component_event_writer.send(bevy_events::InsertComponentEvents::new(inserts));
            }

            // Update Component Event
            if events.has_updates() {
                let updates = events.take_updates().unwrap();
                let mut update_component_event_writer = world
                    .get_resource_mut::<Events<bevy_events::UpdateComponentEvents>>()
                    .unwrap();
                update_component_event_writer
                    .send(bevy_events::UpdateComponentEvents::new(updates));
            }

            // Remove Component Event
            if events.has_removes() {
                let removes = events.take_removes().unwrap();
                let mut remove_component_event_writer = world
                    .get_resource_mut::<Events<bevy_events::RemoveComponentEvents>>()
                    .unwrap();

                remove_component_event_writer.send(bevy_events::RemoveComponentEvents::new(removes));
            }

            if did_tick {
                server.send_all_updates(world.proxy());
            }
        }
    });
}
