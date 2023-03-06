use std::ops::DerefMut;

use bevy_ecs::{
    entity::Entity,
    event::Events,
    world::{Mut, World},
};

use naia_bevy_shared::{
    events::BevyWorldEvents, HostComponentEvent, WorldMutType, WorldProxy, WorldProxyMut,
};
use naia_server::Server;

use crate::ClientOwned;

mod naia_events {
    pub use naia_server::{ConnectEvent, DisconnectEvent, ErrorEvent, TickEvent};
}

mod bevy_events {
    pub use crate::events::{
        AuthEvents, ConnectEvent, DisconnectEvent, ErrorEvent, MessageEvents, TickEvent,
        UpdateComponentEvents,
    };
}

pub fn before_receive_events(world: &mut World) {
    world.resource_scope(|world, mut server: Mut<Server<Entity>>| {
        if !server.is_listening() {
            return;
        }

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
                server.insert_component_worldless(&entity, DerefMut::deref_mut(&mut component_mut));
            } else {
                server.remove_component_worldless(&entity, &component_kind);
            }
        }

        // Receive Events
        let mut did_tick = false;
        let mut events = server.receive(world.proxy_mut());
        if !events.is_empty() {

            // Connect Event
            if events.has::<naia_events::ConnectEvent>() {
                let mut connect_event_writer = world_cell
                    .get_resource_mut::<Events<bevy_events::ConnectEvent>>()
                    .unwrap();
                for user_key in events.read::<naia_events::ConnectEvent>() {
                    connect_event_writer.send(bevy_events::ConnectEvent(user_key));
                }
            }

            // Disconnect Event
            if events.has::<naia_events::DisconnectEvent>() {
                let mut disconnect_event_writer = world_cell
                    .get_resource_mut::<Events<bevy_events::DisconnectEvent>>()
                    .unwrap();
                for (user_key, user) in events.read::<naia_events::DisconnectEvent>() {
                    disconnect_event_writer.send(bevy_events::DisconnectEvent(user_key, user));
                }
            }

            // Error Event
            if events.has::<naia_events::ErrorEvent>() {
                let mut error_event_writer = world_cell
                    .get_resource_mut::<Events<bevy_events::ErrorEvent>>()
                    .unwrap();
                for error in events.read::<naia_events::ErrorEvent>() {
                    error_event_writer.send(bevy_events::ErrorEvent(error));
                }
            }

            // Tick Event
            if events.has::<naia_events::TickEvent>() {
                let mut tick_event_writer = world_cell
                    .get_resource_mut::<Events<bevy_events::TickEvent>>()
                    .unwrap();
                for tick in events.read::<naia_events::TickEvent>() {
                    tick_event_writer.send(bevy_events::TickEvent(tick));
                    did_tick = true;
                }
            }

            // Message Event
            if events.has::<naia_events::MessageEvents>() {
                let mut message_event_writer = world_cell
                    .get_resource_mut::<Events<bevy_events::MessageEvents>>()
                    .unwrap();
                message_event_writer.send(bevy_events::MessageEvents::from(&mut events));
            }

            // Auth Event
            if events.has::<naia_events::AuthEvents>() {
                let mut auth_event_writer = world_cell
                    .get_resource_mut::<Events<bevy_events::AuthEvents>>()
                    .unwrap();
                auth_event_writer.send(bevy_events::AuthEvents::from(&mut events));
            }

            // Update Component Event
            if events.has::<naia_events::UpdateComponentEvents>() {
                let updates = events.world.take_updates().unwrap();
                let mut update_component_event_writer = world_cell
                    .get_resource_mut::<Events<bevy_events::UpdateComponentEvents>>()
                    .unwrap();
                update_component_event_writer
                    .send(bevy_events::UpdateComponentEvents::new(updates));
            }

            // Spawn Entity Event
            if events.has::<naia_events::SpawnEntityEvent>() {
                let mut spawn_entity_event_writer = world
                    .get_resource_mut::<Events<bevy_events::SpawnEntityEvent>>()
                    .unwrap();
                let mut spawned_entities = Vec::new();
                for entity in world_events.read::<naia_events::SpawnEntityEvent>() {
                    spawned_entities.push(entity);
                    spawn_entity_event_writer.send(bevy_events::SpawnEntityEvent(entity));
                }
                for entity in spawned_entities {
                    let user_key = server.entity_owner(&entity);
                    world.entity_mut(entity).insert(ClientOwned(user_key));
                }
            }

            // Despawn, Insert, Remove Events
            BevyWorldEvents::write_events(&mut events.world, world);

            if did_tick {
                server.send_all_updates(world.proxy());
            }
        }
    });
}
