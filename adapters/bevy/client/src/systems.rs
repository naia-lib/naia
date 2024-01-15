use std::ops::DerefMut;

use log::warn;

use bevy_ecs::{
    event::Events,
    world::{Mut, World},
};

use naia_bevy_shared::{HostOwned, HostSyncEvent, WorldMutType, WorldProxyMut};

mod naia_events {
    pub use naia_client::{
        ClientTickEvent, ConnectEvent, DespawnEntityEvent, DisconnectEvent, EntityAuthDeniedEvent,
        EntityAuthGrantedEvent, EntityAuthResetEvent, ErrorEvent, PublishEntityEvent, RejectEvent,
        ServerTickEvent, SpawnEntityEvent, UnpublishEntityEvent,
    };
}

mod bevy_events {
    pub use crate::events::{
        ClientTickEvent, ConnectEvent, DespawnEntityEvent, DisconnectEvent, EntityAuthDeniedEvent,
        EntityAuthGrantedEvent, EntityAuthResetEvent, ErrorEvent, InsertComponentEvents,
        MessageEvents, PublishEntityEvent, RejectEvent, RemoveComponentEvents, ServerTickEvent,
        SpawnEntityEvent, UnpublishEntityEvent, UpdateComponentEvents,
    };
}

use crate::{ServerOwned, client::ClientWrapper};

pub fn before_receive_events<T: Send + Sync + 'static>(world: &mut World) {
    world.resource_scope(|world, mut client: Mut<ClientWrapper<T>>| {

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
                    client.client.insert_component_worldless(&entity, DerefMut::deref_mut(&mut component_mut));
                }
                HostSyncEvent::Remove(entity, component_kind) => {
                    client.client.remove_component_worldless(&entity, &component_kind);
                }
                HostSyncEvent::Despawn(entity) => {
                    client.client.despawn_entity_worldless(&entity);
                }
            }
        }

        // Receive Events
        let mut events = client.client.receive(world.proxy_mut());
        if !events.is_empty() {

            if events.has::<naia_events::ConnectEvent>() {
                // Connect Event
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::ConnectEvent<T>>>()
                    .unwrap();
                for _ in events.read::<naia_events::ConnectEvent>() {
                    event_writer.send(bevy_events::ConnectEvent::<T>::new());
                }
            }

            // Disconnect Event
            if events.has::<naia_events::DisconnectEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::DisconnectEvent<T>>>()
                    .unwrap();
                for _ in events.read::<naia_events::DisconnectEvent>() {
                    event_writer.send(bevy_events::DisconnectEvent::<T>::new());
                }
            }

            // Reject Event
            if events.has::<naia_events::RejectEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::RejectEvent<T>>>()
                    .unwrap();
                for _ in events.read::<naia_events::RejectEvent>() {
                    event_writer.send(bevy_events::RejectEvent::<T>::new());
                }
            }

            // Error Event
            if events.has::<naia_events::ErrorEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::ErrorEvent<T>>>()
                    .unwrap();
                for error in events.read::<naia_events::ErrorEvent>() {
                    event_writer.send(bevy_events::ErrorEvent::<T>::new(error));
                }
            }

            // Client Tick Event
            if events.has::<naia_events::ClientTickEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::ClientTickEvent<T>>>()
                    .unwrap();
                for tick in events.read::<naia_events::ClientTickEvent>() {
                    event_writer.send(bevy_events::ClientTickEvent::<T>::new(tick));
                }
            }

            // Server Tick Event
            if events.has::<naia_events::ServerTickEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::ServerTickEvent<T>>>()
                    .unwrap();
                for tick in events.read::<naia_events::ServerTickEvent>() {
                    event_writer.send(bevy_events::ServerTickEvent::<T>::new(tick));
                }
            }

            // Message Event
            if events.has_messages() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::MessageEvents<T>>>()
                    .unwrap();
                event_writer.send(bevy_events::MessageEvents::from(&mut events));
            }

            // Spawn Entity Event
            if events.has::<naia_events::SpawnEntityEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::SpawnEntityEvent<T>>>()
                    .unwrap();

                let mut spawned_entities = Vec::new();
                for entity in events.read::<naia_events::SpawnEntityEvent>() {
                    spawned_entities.push(entity);
                    event_writer.send(bevy_events::SpawnEntityEvent::<T>::new(entity));
                }
                for entity in spawned_entities {
                    world.entity_mut(entity).insert(ServerOwned);
                }
            }

            // Despawn Entity Event
            if events.has::<naia_events::DespawnEntityEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::DespawnEntityEvent<T>>>()
                    .unwrap();
                for entity in events.read::<naia_events::DespawnEntityEvent>() {
                    event_writer.send(bevy_events::DespawnEntityEvent::<T>::new(entity));
                }
            }

            // Publish Entity Event
            if events.has::<naia_events::PublishEntityEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::PublishEntityEvent<T>>>()
                    .unwrap();
                for entity in events.read::<naia_events::PublishEntityEvent>() {
                    event_writer.send(bevy_events::PublishEntityEvent::<T>::new(entity));
                }
            }

            // Unpublish Entity Event
            if events.has::<naia_events::UnpublishEntityEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::UnpublishEntityEvent<T>>>()
                    .unwrap();
                for entity in events.read::<naia_events::UnpublishEntityEvent>() {
                    event_writer.send(bevy_events::UnpublishEntityEvent::<T>::new(entity));
                }
            }

            // Entity Auth Granted Event
            if events.has::<naia_events::EntityAuthGrantedEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::EntityAuthGrantedEvent<T>>>()
                    .unwrap();
                let mut auth_granted_entities = Vec::new();
                for entity in events.read::<naia_events::EntityAuthGrantedEvent>() {
                    auth_granted_entities.push(entity);
                    event_writer.send(bevy_events::EntityAuthGrantedEvent::<T>::new(entity));
                }
                for entity in auth_granted_entities {
                    if world.get_entity(entity).is_some() {
                        world.entity_mut(entity).insert(HostOwned::<T>::new());
                    } else {
                        warn!("Granted auth to an entity that no longer exists! {:?}", entity);
                    }
                }
            }

            // Entity Auth Denied Event
            if events.has::<naia_events::EntityAuthDeniedEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::EntityAuthDeniedEvent<T>>>()
                    .unwrap();
                for entity in events.read::<naia_events::EntityAuthDeniedEvent>() {
                    event_writer.send(bevy_events::EntityAuthDeniedEvent::<T>::new(entity));
                }
            }

            // Entity Auth Reset Event
            if events.has::<naia_events::EntityAuthResetEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::EntityAuthResetEvent<T>>>()
                    .unwrap();
                let mut auth_reset_entities = Vec::new();
                for entity in events.read::<naia_events::EntityAuthResetEvent>() {
                    auth_reset_entities.push(entity);
                    event_writer.send(bevy_events::EntityAuthResetEvent::<T>::new(entity));
                }
                for entity in auth_reset_entities {
                    if world.get_entity(entity).is_some() {
                        world.entity_mut(entity).remove::<HostOwned<T>>();
                    } else {
                        warn!("Reset auth to an entity that no longer exists! {:?}", entity);
                    }
                }
            }

            // Insert Component Event
            if events.has_inserts() {
                let inserts = events.take_inserts().unwrap();
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::InsertComponentEvents<T>>>()
                    .unwrap();
                event_writer.send(bevy_events::InsertComponentEvents::<T>::new(inserts));
            }

            // Update Component Event
            if events.has_updates() {
                let updates = events.take_updates().unwrap();
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::UpdateComponentEvents<T>>>()
                    .unwrap();
                event_writer
                    .send(bevy_events::UpdateComponentEvents::<T>::new(updates));
            }

            // Remove Component Event
            if events.has_removes() {
                let removes = events.take_removes().unwrap();
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::RemoveComponentEvents<T>>>()
                    .unwrap();

                event_writer.send(bevy_events::RemoveComponentEvents::<T>::new(removes));
            }
        }
    });
}
