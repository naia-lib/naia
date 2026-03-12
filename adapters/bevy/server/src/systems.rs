use std::ops::DerefMut;

use bevy_ecs::{
    message::Messages,
    system::{Res, ResMut, SystemState},
    world::{Mut, World},
};

use log::warn;

use naia_bevy_shared::{
    HostOwned, HostSyncEvent, Instant, WorldMutType, WorldProxy, WorldProxyMut,
};
use naia_server::EntityOwner;

use crate::{
    component_event_registry::ComponentEventRegistry, plugin::Singleton, server::ServerImpl,
    ClientOwned, EntityAuthStatus,
};

mod naia_events {
    pub use naia_server::{
        ConnectEvent, DelegateEntityEvent, DespawnEntityEvent, DisconnectEvent,
        EntityAuthGrantEvent, EntityAuthResetEvent, ErrorEvent, PublishEntityEvent,
        SpawnEntityEvent, TickEvent, UnpublishEntityEvent,
    };
}

mod bevy_events {
    pub use crate::events::{
        AuthEvents, ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent, MessageEvents,
        PublishEntityEvent, RequestEvents, SpawnEntityEvent, TickEvent, UnpublishEntityEvent,
    };
}

use crate::events::CachedTickEventsState;

pub fn world_to_host_sync(world: &mut World) {
    world.resource_scope(|world, mut server: Mut<ServerImpl>| {
        if !server.is_listening() {
            return;
        }

        // Host Component Updates
        let mut host_component_event_reader =
            world.get_resource_mut::<Messages<HostSyncEvent>>().unwrap();
        let host_component_events: Vec<HostSyncEvent> =
            host_component_event_reader.drain().collect();
        for event in host_component_events {
            match event {
                HostSyncEvent::Insert(_host_id, entity, component_kind) => {
                    if server.entity_authority_status(world.proxy(), &entity)
                        == Some(EntityAuthStatus::Denied)
                    {
                        // if auth status is denied, that means the client is performing this operation and it's already being handled
                        continue;
                    }
                    let mut world_proxy = world.proxy_mut();
                    let Some(mut component_mut) =
                        world_proxy.component_mut_of_kind(&entity, &component_kind)
                    else {
                        warn!("could not find Component in World which has just been inserted!");
                        continue;
                    };
                    server.insert_component_worldless(
                        &entity,
                        DerefMut::deref_mut(&mut component_mut),
                    );
                }
                HostSyncEvent::Remove(_host_id, entity, component_kind) => {
                    if server.entity_authority_status(world.proxy(), &entity)
                        == Some(EntityAuthStatus::Denied)
                    {
                        // if auth status is denied, that means the client is performing this operation and it's already being handled
                        continue;
                    }
                    server.remove_component_worldless(&entity, &component_kind);
                }
                HostSyncEvent::Despawn(_host_id, entity) => {
                    if server.entity_authority_status(world.proxy(), &entity)
                        == Some(EntityAuthStatus::Denied)
                    {
                        // if auth status is denied, that means the client is performing this operation and it's already being handled
                        continue;
                    }
                    server.despawn_entity_worldless(&entity);
                }
            }
        }
    });
}

pub fn receive_packets(mut server: ResMut<ServerImpl>) {
    if !server.is_listening() {
        return;
    }

    server.receive_all_packets();
}

pub fn translate_tick_events(
    mut server: ResMut<ServerImpl>,
    mut tick_events: ResMut<Messages<bevy_events::TickEvent>>,
) {
    if !server.is_listening() {
        return;
    }

    let now = Instant::now();

    // Receive Events
    let mut events = server.take_tick_events(&now);
    if !events.is_empty() {
        // Tick Event
        if events.has::<naia_events::TickEvent>() {
            for tick in events.read::<naia_events::TickEvent>() {
                tick_events.write(bevy_events::TickEvent(tick));
            }
        }
    }
}

pub fn process_packets(world: &mut World) {
    world.resource_scope(|world, mut server: Mut<ServerImpl>| {
        if !server.is_listening() {
            return;
        }

        let now = Instant::now();
        server.process_all_packets(world.proxy_mut(), &now);
    });
}

pub fn translate_world_events(world: &mut World) {
    world.resource_scope(|world, mut server: Mut<ServerImpl>| {
        if !server.is_listening() {
            return;
        }

        // Receive Events
        let mut events = server.take_world_events();
        if !events.is_empty() {
            // Connect Event
            if events.has::<naia_events::ConnectEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Messages<bevy_events::ConnectEvent>>()
                    .unwrap();
                for user_key in events.read::<naia_events::ConnectEvent>() {
                    event_writer.write(bevy_events::ConnectEvent(user_key));
                }
            }

            // Disconnect Event
            if events.has::<naia_events::DisconnectEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Messages<bevy_events::DisconnectEvent>>()
                    .unwrap();
                for (user_key, user) in events.read::<naia_events::DisconnectEvent>() {
                    event_writer.write(bevy_events::DisconnectEvent(user_key, user));
                }
            }

            // Error Event
            if events.has::<naia_events::ErrorEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Messages<bevy_events::ErrorEvent>>()
                    .unwrap();
                for error in events.read::<naia_events::ErrorEvent>() {
                    event_writer.write(bevy_events::ErrorEvent(error));
                }
            }

            // Message Event
            if events.has_messages() {
                let mut event_writer = world
                    .get_resource_mut::<Messages<bevy_events::MessageEvents>>()
                    .unwrap();
                event_writer.write(bevy_events::MessageEvents::from(&mut events));
            }

            // Request Event
            if events.has_requests() {
                let mut event_writer = world
                    .get_resource_mut::<Messages<bevy_events::RequestEvents>>()
                    .unwrap();
                event_writer.write(bevy_events::RequestEvents::from(&mut events));
            }

            // Auth Event
            if events.has_auths() {
                let mut event_writer = world
                    .get_resource_mut::<Messages<bevy_events::AuthEvents>>()
                    .unwrap();
                event_writer.write(bevy_events::AuthEvents::from(&mut events));
            }

            // Spawn Entity Event
            if events.has::<naia_events::SpawnEntityEvent>() {
                let mut client_spawned_entities = Vec::new();
                for (_, entity) in events.read::<naia_events::SpawnEntityEvent>() {
                    if let EntityOwner::Client(user_key) =
                        server.entity_owner(world.proxy(), &entity)
                    {
                        client_spawned_entities.push((user_key, entity));
                    }
                }
                {
                    let mut event_writer = world
                        .get_resource_mut::<Messages<bevy_events::SpawnEntityEvent>>()
                        .unwrap();
                    for &(user_key, entity) in &client_spawned_entities {
                        event_writer.write(bevy_events::SpawnEntityEvent(user_key, entity));
                    }
                }
                for (user_key, entity) in client_spawned_entities {
                    world.entity_mut(entity).insert(ClientOwned(user_key));
                }
            }

            // Despawn Entity Event
            if events.has::<naia_events::DespawnEntityEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Messages<bevy_events::DespawnEntityEvent>>()
                    .unwrap();
                for (user_key, entity) in events.read::<naia_events::DespawnEntityEvent>() {
                    event_writer.write(bevy_events::DespawnEntityEvent(user_key, entity));
                }
            }

            // Publish Entity Event
            if events.has::<naia_events::PublishEntityEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Messages<bevy_events::PublishEntityEvent>>()
                    .unwrap();
                for (user_key, entity) in events.read::<naia_events::PublishEntityEvent>() {
                    event_writer.write(bevy_events::PublishEntityEvent(user_key, entity));
                }
            }

            // Unpublish Entity Event
            if events.has::<naia_events::UnpublishEntityEvent>() {
                let mut event_writer = world
                    .get_resource_mut::<Messages<bevy_events::UnpublishEntityEvent>>()
                    .unwrap();
                for (user_key, entity) in events.read::<naia_events::UnpublishEntityEvent>() {
                    event_writer.write(bevy_events::UnpublishEntityEvent(user_key, entity));
                }
            }

            // Delegate Entity Event
            if events.has::<naia_events::DelegateEntityEvent>() {
                for (_, entity) in events.read::<naia_events::DelegateEntityEvent>() {
                    world
                        .entity_mut(entity)
                        .insert(HostOwned::new::<Singleton>());
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
                    if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
                        entity_mut.insert(HostOwned::new::<Singleton>());
                    }
                }
            }

            world.resource_scope(|world, mut registry: Mut<ComponentEventRegistry>| {
                registry.receive_events(world, &mut events);
            });
        }
    });
}

pub fn send_packets_init(world: &mut World) {
    let tick_event_state: SystemState<(
        Res<Messages<bevy_events::TickEvent>>,
        bevy_ecs::system::Local<bevy_ecs::message::MessageCursor<bevy_events::TickEvent>>,
    )> = SystemState::new(world);
    world.insert_resource(CachedTickEventsState {
        event_state: tick_event_state,
    });
}

pub fn send_packets(world: &mut World) {
    world.resource_scope(|world, mut server: Mut<ServerImpl>| {
        if !server.is_listening() {
            return;
        }

        world.resource_scope(
            |world, mut events_reader_state: Mut<CachedTickEventsState>| {
                // Tick Event
                let mut did_tick = false;

                let (messages, mut cursor) = events_reader_state.event_state.get_mut(world);

                for bevy_events::TickEvent(_tick) in cursor.read(&messages) {
                    did_tick = true;
                }

                if did_tick {
                    server.send_all_packets(world.proxy());
                }
            },
        );
    });
}
