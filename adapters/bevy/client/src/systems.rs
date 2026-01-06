use std::{any::TypeId, ops::DerefMut};

use log::{info, warn};

use bevy_ecs::{
    event::{EventReader, EventWriter, Events},
    system::{ResMut, SystemState},
    world::{Mut, World},
};

use naia_bevy_shared::{
    HostOwned, HostSyncEvent, Instant, WorldMutType, WorldProxy, WorldProxyMut,
};

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
        EntityAuthGrantedEvent, EntityAuthResetEvent, ErrorEvent, MessageEvents,
        PublishEntityEvent, RejectEvent, RequestEvents, ServerTickEvent, SpawnEntityEvent,
        UnpublishEntityEvent,
    };
}

use crate::{
    client::ClientWrapper, component_event_registry::ComponentEventRegistry,
    events::CachedClientTickEventsState, ServerOwned,
};

pub fn world_to_host_sync<T: Send + Sync + 'static>(world: &mut World) {
    let host_id = TypeId::of::<T>();

    world.resource_scope(|world, mut client: Mut<ClientWrapper<T>>| {
        // Host Component Updates
        let mut other_host_component_events = Vec::new();
        let mut host_component_event_reader =
            world.get_resource_mut::<Events<HostSyncEvent>>().unwrap();
        let host_component_events: Vec<HostSyncEvent> =
            host_component_event_reader.drain().collect();
        for event in host_component_events {
            if event.host_id() != host_id {
                other_host_component_events.push(event);
                continue;
            }
            match event {
                HostSyncEvent::Insert(_, entity, component_kind) => {
                    let mut world_proxy = world.proxy_mut();
                    let Some(mut component_mut) =
                        world_proxy.component_mut_of_kind(&entity, &component_kind)
                    else {
                        // let component_name = client.client.component_name(&component_kind);
                        // warn!(
                        //     "Tried to insert component {:?} on entity {:?}, but it does not exist!",
                        //     component_name, entity
                        // );
                        continue;
                    };
                    client.client.insert_component_worldless(
                        &entity,
                        DerefMut::deref_mut(&mut component_mut),
                    );
                }
                HostSyncEvent::Remove(_, entity, component_kind) => {
                    client
                        .client
                        .remove_component_worldless(&entity, &component_kind);
                }
                HostSyncEvent::Despawn(_, entity) => {
                    info!("despawn on HostOwned entity: {:?}", entity);
                    client.client.despawn_entity_worldless(&entity);
                }
            }
        }

        // pass non-matching host component events to be handled elsewhere
        if !other_host_component_events.is_empty() {
            let mut event_writer = world.get_resource_mut::<Events<HostSyncEvent>>().unwrap();
            for event in other_host_component_events {
                event_writer.send(event);
            }
        }
    });
}

pub fn receive_packets<T: Send + Sync + 'static>(mut client: ResMut<ClientWrapper<T>>) {
    client.client.receive_all_packets();
}

pub fn translate_tick_events<T: Send + Sync + 'static>(
    mut client: ResMut<ClientWrapper<T>>,
    mut server_tick_event_writer: EventWriter<bevy_events::ServerTickEvent<T>>,
    mut client_tick_event_writer: EventWriter<bevy_events::ClientTickEvent<T>>,
) {
    let now = Instant::now();

    // Receive Events
    let mut events = client.client.take_tick_events(&now);
    if !events.is_empty() {
        // Client Tick Event
        if events.has::<naia_events::ClientTickEvent>() {
            for tick in events.read::<naia_events::ClientTickEvent>() {
                client_tick_event_writer.write(bevy_events::ClientTickEvent::<T>::new(tick));
            }
        }

        // Server Tick Event
        if events.has::<naia_events::ServerTickEvent>() {
            for tick in events.read::<naia_events::ServerTickEvent>() {
                server_tick_event_writer.write(bevy_events::ServerTickEvent::<T>::new(tick));
            }
        }
    }
}

pub fn process_packets<T: Send + Sync + 'static>(world: &mut World) {
    world.resource_scope(|world, mut client: Mut<ClientWrapper<T>>| {
        let now = Instant::now();
        client.client.process_all_packets(world.proxy_mut(), &now);
    });
}

pub fn translate_world_events<T: Send + Sync + 'static>(world: &mut World) {
    world.resource_scope(|world, mut client: Mut<ClientWrapper<T>>| {
        // Receive Events
        let mut events = client.client.take_world_events();
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

            // Message Event
            if events.has_messages() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::MessageEvents<T>>>()
                    .unwrap();
                event_writer.send(bevy_events::MessageEvents::from(&mut events));
            }

            // Request Event
            if events.has_requests() {
                let mut event_writer = world
                    .get_resource_mut::<Events<bevy_events::RequestEvents<T>>>()
                    .unwrap();
                event_writer.send(bevy_events::RequestEvents::from(&mut events));
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
                    if world.get_entity(entity).is_ok() {
                        world.entity_mut(entity).insert(HostOwned::new::<T>());
                    } else {
                        warn!(
                            "Granted auth to an entity that no longer exists! {:?}",
                            entity
                        );
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
                    if world.get_entity(entity).is_ok() {
                        world.entity_mut(entity).remove::<HostOwned>();
                    } else {
                        warn!(
                            "Reset auth to an entity that no longer exists! {:?}",
                            entity
                        );
                    }
                }
            }

            world.resource_scope(|world, mut registry: Mut<ComponentEventRegistry<T>>| {
                registry.receive_events(world, &mut events);
            });
        }
    });
}

pub fn send_packets_init<T: Send + Sync + 'static>(world: &mut World) {
    let tick_event_state: SystemState<EventReader<bevy_events::ClientTickEvent<T>>> =
        SystemState::new(world);
    world.insert_resource(CachedClientTickEventsState {
        event_state: tick_event_state,
    });
}

pub fn send_packets<T: Send + Sync + 'static>(world: &mut World) {
    world.resource_scope(|world, mut client: Mut<ClientWrapper<T>>| {
        // if disconnected, always send
        let mut should_send = if client.client.connection_status().is_connected() {
            false
        } else {
            true
        };

        // if connected, check if we have ticked before sending packets
        if !should_send {
            world.resource_scope(
                |world, mut events_reader_state: Mut<CachedClientTickEventsState<T>>| {
                    let mut events_reader = events_reader_state.event_state.get_mut(world);

                    for _event in events_reader.read() {
                        should_send = true;
                    }
                },
            );
        }

        // send packets
        if should_send {
            client.client.send_all_packets(world.proxy());
        }
    });
}
