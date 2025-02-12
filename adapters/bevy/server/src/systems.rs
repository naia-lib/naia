
use std::{ops::DerefMut, collections::HashMap};

use bevy_ecs::{
    event::{EventReader, Events},
    system::SystemState,
    world::{Mut, World},
};

use log::warn;

use naia_bevy_shared::{HostOwned, HostSyncEvent, WorldMutType};
use naia_server::EntityOwner;

use crate::{world_proxy::{get_world_mut_from_id, WorldProxyMut, WorldProxy}, world_entity::WorldId, plugin::Singleton, server::ServerWrapper, ClientOwned, EntityAuthStatus};

mod naia_events {
    pub use naia_server::{
        ConnectEvent, DelegateEntityEvent, DespawnEntityEvent, DisconnectEvent,
        EntityAuthGrantEvent, EntityAuthResetEvent, ErrorEvent, PublishEntityEvent,
        SpawnEntityEvent, TickEvent, UnpublishEntityEvent,
    };
}

mod bevy_events {
    pub use crate::events::{
        AuthEvents, ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent,
        InsertComponentEvents, MessageEvents, PublishEntityEvent, RemoveComponentEvents,
        RequestEvents, SpawnEntityEvent, TickEvent, UnpublishEntityEvent, UpdateComponentEvents,
    };
}

use crate::events::CachedTickEventsState;
use crate::sub_worlds::SubWorlds;
use crate::world_entity::WorldEntity;

pub fn before_receive_events(main_world: &mut World) {
    main_world.resource_scope(|main_world, mut server: Mut<ServerWrapper>| {
        if !server.is_listening() {
            return;
        }

        // Host Component Updates in Main World
        host_component_updates(WorldId::main(), &mut server, main_world);

        // Host Component Updates in Subworlds
        let mut sub_worlds = main_world.get_resource_mut::<SubWorlds>().unwrap();
        for (world_id, sub_world) in sub_worlds.iter_mut() {
            host_component_updates(world_id, &mut server, sub_world);
        }

        // Receive Events
        before_receive_events_impl(main_world, &mut server);
    });
}

fn host_component_updates(world_id: WorldId, server: &mut ServerWrapper, sub_world: &mut World) {
    // Host Component Updates
    let mut host_component_event_reader = sub_world
        .get_resource_mut::<Events<HostSyncEvent>>()
        .unwrap();
    let host_component_events: Vec<HostSyncEvent> = host_component_event_reader.drain().collect();
    for event in host_component_events {
        match event {
            HostSyncEvent::Insert(_host_id, entity, component_kind) => {
                let world_entity = WorldEntity::new(world_id, entity);
                if server.entity_authority_status(&world_entity) == Some(EntityAuthStatus::Denied) {
                    // if auth status is denied, that means the client is performing this operation and it's already being handled
                    continue;
                }
                let mut world_proxy = sub_world.proxy_mut();
                let Some(mut component_mut) = world_proxy.component_mut_of_kind(&world_entity, &component_kind) else {
                    warn!("could not find Component in World which has just been inserted!");
                    continue;
                };
                server.inner_mut().insert_component_worldless(&world_entity, DerefMut::deref_mut(&mut component_mut));
            }
            HostSyncEvent::Remove(_host_id, entity, component_kind) => {
                let world_entity = WorldEntity::new(world_id, entity);
                if server.entity_authority_status(&world_entity) == Some(EntityAuthStatus::Denied) {
                    // if auth status is denied, that means the client is performing this operation and it's already being handled
                    continue;
                }
                server.inner_mut().remove_component_worldless(&world_entity, &component_kind);
            }
            HostSyncEvent::Despawn(_host_id, entity) => {
                let world_entity = WorldEntity::new(world_id, entity);
                if server.entity_authority_status(&world_entity) == Some(EntityAuthStatus::Denied) {
                    // if auth status is denied, that means the client is performing this operation and it's already being handled
                    continue;
                }
                server.inner_mut().despawn_entity_worldless(&world_entity);
            }
        }
    }
}

fn before_receive_events_impl(main_world: &mut World, server: &mut ServerWrapper) {

    // Receive Events
    let mut events = server.inner_mut().receive(main_world.proxy_mut());
    if !events.is_empty() {

        // Connect Event
        if events.has::<naia_events::ConnectEvent>() {
            let mut event_writer = main_world
                .get_resource_mut::<Events<bevy_events::ConnectEvent>>()
                .unwrap();
            for user_key in events.read::<naia_events::ConnectEvent>() {
                event_writer.send(bevy_events::ConnectEvent(user_key));
            }
        }

        // Disconnect Event
        if events.has::<naia_events::DisconnectEvent>() {
            let mut event_writer = main_world
                .get_resource_mut::<Events<bevy_events::DisconnectEvent>>()
                .unwrap();
            for (user_key, user) in events.read::<naia_events::DisconnectEvent>() {
                event_writer.send(bevy_events::DisconnectEvent(user_key, user));
            }
        }

        // Error Event
        if events.has::<naia_events::ErrorEvent>() {
            let mut event_writer = main_world
                .get_resource_mut::<Events<bevy_events::ErrorEvent>>()
                .unwrap();
            for error in events.read::<naia_events::ErrorEvent>() {
                event_writer.send(bevy_events::ErrorEvent(error));
            }
        }

        // Tick Event
        if events.has::<naia_events::TickEvent>() {
            let mut event_writer = main_world
                .get_resource_mut::<Events<bevy_events::TickEvent>>()
                .unwrap();
            for tick in events.read::<naia_events::TickEvent>() {
                event_writer.send(bevy_events::TickEvent(tick));
            }
        }

        // Message Event
        if events.has_messages() {
            let mut event_writer = main_world
                .get_resource_mut::<Events<bevy_events::MessageEvents>>()
                .unwrap();
            event_writer.send(bevy_events::MessageEvents::from(&mut events));
        }

        // Request Event
        if events.has_requests() {
            let mut event_writer = main_world
                .get_resource_mut::<Events<bevy_events::RequestEvents>>()
                .unwrap();
            event_writer.send(bevy_events::RequestEvents::from(&mut events));
        }

        // Auth Event
        if events.has_auths() {
            let mut event_writer = main_world
                .get_resource_mut::<Events<bevy_events::AuthEvents>>()
                .unwrap();
            event_writer.send(bevy_events::AuthEvents::from(&mut events));
        }

        // Spawn Entity Event
        if events.has::<naia_events::SpawnEntityEvent>() {
            let mut spawned_world_entities = Vec::new();
            {
                let mut event_writer = main_world
                    .get_resource_mut::<Events<bevy_events::SpawnEntityEvent>>()
                    .unwrap();

                for (new_user_key, world_entity) in events.read::<naia_events::SpawnEntityEvent>() {
                    let world_id = world_entity.world_id();
                    let entity = world_entity.entity();
                    spawned_world_entities.push((new_user_key, world_entity, entity));
                    event_writer.send(bevy_events::SpawnEntityEvent(new_user_key, world_id, entity));
                }
            }
            for (new_user_key, world_entity, entity) in spawned_world_entities {
                let EntityOwner::Client(existing_user_key) = server.inner().entity_owner(&world_entity) else {
                    panic!("spawned entity that doesn't belong to a client ... shouldn't be possible.");
                };
                if new_user_key != existing_user_key {
                    panic!("spawned entity that already belongs to a different client ... shouldn't be possible.");
                }

                get_world_mut_from_id(main_world, &world_entity, |world| {
                    world.entity_mut(entity).insert(ClientOwned(new_user_key));
                });
            }
        }

        // Despawn Entity Event
        if events.has::<naia_events::DespawnEntityEvent>() {
            let mut event_writer = main_world
                .get_resource_mut::<Events<bevy_events::DespawnEntityEvent>>()
                .unwrap();
            for (user_key, world_entity) in events.read::<naia_events::DespawnEntityEvent>() {
                let world_id = world_entity.world_id();
                let entity = world_entity.entity();
                event_writer.send(bevy_events::DespawnEntityEvent(user_key, world_id, entity));
            }
        }

        // Publish Entity Event
        if events.has::<naia_events::PublishEntityEvent>() {
            let mut event_writer = main_world
                .get_resource_mut::<Events<bevy_events::PublishEntityEvent>>()
                .unwrap();
            for (user_key, world_entity) in events.read::<naia_events::PublishEntityEvent>() {
                let world_id = world_entity.world_id();
                let entity = world_entity.entity();
                event_writer.send(bevy_events::PublishEntityEvent(user_key, world_id, entity));
            }
        }

        // Unpublish Entity Event
        if events.has::<naia_events::UnpublishEntityEvent>() {
            let mut event_writer = main_world
                .get_resource_mut::<Events<bevy_events::UnpublishEntityEvent>>()
                .unwrap();
            for (user_key, world_entity) in events.read::<naia_events::UnpublishEntityEvent>() {
                let world_id = world_entity.world_id();
                let entity = world_entity.entity();
                event_writer.send(bevy_events::UnpublishEntityEvent(user_key, world_id, entity));
            }
        }

        // Delegate Entity Event
        if events.has::<naia_events::DelegateEntityEvent>() {
            for (_, world_entity) in events.read::<naia_events::DelegateEntityEvent>() {

                let entity = world_entity.entity();

                get_world_mut_from_id(main_world, &world_entity, |world| {
                    world.entity_mut(entity).insert(HostOwned::new::<Singleton>());
                });
            }
        }

        // Entity Auth Given Event
        if events.has::<naia_events::EntityAuthGrantEvent>() {
            for (_, world_entity) in events.read::<naia_events::EntityAuthGrantEvent>() {

                let entity = world_entity.entity();

                get_world_mut_from_id(main_world, &world_entity, |world| {
                    world.entity_mut(entity).remove::<HostOwned>();
                });
            }
        }

        // Entity Auth Reset Event
        if events.has::<naia_events::EntityAuthResetEvent>() {
            for world_entity in events.read::<naia_events::EntityAuthResetEvent>() {

                let entity = world_entity.entity();

                get_world_mut_from_id(main_world, &world_entity, |world| {
                    if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
                        entity_mut.insert(HostOwned::new::<Singleton>());
                    }
                });
            }
        }

        // Insert Component Event
        if events.has_inserts() {
            let inserts = events.take_inserts().unwrap();
            let mut new_inserts = HashMap::new();
            for (kind, components) in inserts {
                let mut new_components = Vec::new();
                for (user_key, world_entity) in components {
                    let world_id = world_entity.world_id();
                    let entity = world_entity.entity();
                    new_components.push((user_key, world_id, entity));
                }
                new_inserts.insert(kind, new_components);
            }
            let mut event_writer = main_world
                .get_resource_mut::<Events<bevy_events::InsertComponentEvents>>()
                .unwrap();
            event_writer.send(bevy_events::InsertComponentEvents::new(new_inserts));
        }

        // Update Component Event
        if events.has_updates() {
            let updates = events.take_updates().unwrap();
            let mut new_updates = HashMap::new();
            for (kind, components) in updates {
                let mut new_components = Vec::new();
                for (user_key, world_entity) in components {
                    let world_id = world_entity.world_id();
                    let entity = world_entity.entity();
                    new_components.push((user_key, world_id, entity));
                }
                new_updates.insert(kind, new_components);
            }
            let mut event_writer = main_world
                .get_resource_mut::<Events<bevy_events::UpdateComponentEvents>>()
                .unwrap();
            event_writer
                .send(bevy_events::UpdateComponentEvents::new(new_updates));
        }

        // Remove Component Event
        if events.has_removes() {
            let removes = events.take_removes().unwrap();
            let mut new_removes = HashMap::new();
            for (kind, components) in removes {
                let mut new_components = Vec::new();
                for (user_key, world_entity, component) in components {
                    let world_id = world_entity.world_id();
                    let entity = world_entity.entity();
                    new_components.push((user_key, world_id, entity, component));
                }
                new_removes.insert(kind, new_components);
            }
            let mut event_writer = main_world
                .get_resource_mut::<Events<bevy_events::RemoveComponentEvents>>()
                .unwrap();
            event_writer.send(bevy_events::RemoveComponentEvents::new(new_removes));
        }
    }
}

pub fn send_packets_init(world: &mut World) {
    let tick_event_state: SystemState<EventReader<bevy_events::TickEvent>> =
        SystemState::new(world);
    world.insert_resource(CachedTickEventsState {
        event_state: tick_event_state,
    });
}

pub fn send_packets(world: &mut World) {
    world.resource_scope(|world, mut server: Mut<ServerWrapper>| {
        if !server.is_listening() {
            return;
        }

        world.resource_scope(
            |world, mut events_reader_state: Mut<CachedTickEventsState>| {
                // Tick Event
                let mut did_tick = false;

                let mut events_reader = events_reader_state.event_state.get_mut(world);

                for bevy_events::TickEvent(_tick) in events_reader.read() {
                    did_tick = true;
                }

                if did_tick {
                    server.inner_mut().send_all_updates(world.proxy());
                }
            },
        );
    });
}
