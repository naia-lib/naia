use std::ops::DerefMut;

use bevy_ecs::{
    entity::Entity,
    event::Events,
    system::Res,
    world::{Mut, World},
};

use naia_bevy_shared::{events::BevyWorldEvents, HostComponentEvent, WorldMutType, WorldProxyMut};
use naia_client::Client;

mod naia_events {
    pub use naia_client::{
        ClientTickEvent, ConnectEvent, DisconnectEvent, ErrorEvent, RejectEvent, ServerTickEvent,
    };
}

mod bevy_events {
    pub use crate::events::{
        ClientTickEvent, ConnectEvent, DisconnectEvent, ErrorEvent, MessageEvents, RejectEvent,
        ServerTickEvent, UpdateComponentEvents,
    };
}

pub fn before_receive_events(world: &mut World) {
    world.resource_scope(|world, mut client: Mut<Client<Entity>>| {
        let mut events = client.receive(world.proxy_mut());
        if !events.is_empty() {
            unsafe {

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
                let world_cell = world.as_unsafe_world_cell();

                // Connect Event
                let mut connect_event_writer = world_cell
                    .get_resource_mut::<Events<bevy_events::ConnectEvent>>()
                    .unwrap();
                for _ in events.read::<naia_events::ConnectEvent>() {
                    connect_event_writer.send(bevy_events::ConnectEvent);
                }

                // Disconnect Event
                let mut disconnect_event_writer = world_cell
                    .get_resource_mut::<Events<bevy_events::DisconnectEvent>>()
                    .unwrap();
                for _ in events.read::<naia_events::DisconnectEvent>() {
                    disconnect_event_writer.send(bevy_events::DisconnectEvent);
                }

                // Reject Event
                let mut reject_event_writer = world_cell
                    .get_resource_mut::<Events<bevy_events::RejectEvent>>()
                    .unwrap();
                for _ in events.read::<naia_events::RejectEvent>() {
                    reject_event_writer.send(bevy_events::RejectEvent);
                }

                // Error Event
                let mut error_event_writer = world_cell
                    .get_resource_mut::<Events<bevy_events::ErrorEvent>>()
                    .unwrap();
                for error in events.read::<naia_events::ErrorEvent>() {
                    error_event_writer.send(bevy_events::ErrorEvent(error));
                }

                // Client Tick Event
                let mut client_tick_event_writer = world_cell
                    .get_resource_mut::<Events<bevy_events::ClientTickEvent>>()
                    .unwrap();
                for tick in events.read::<naia_events::ClientTickEvent>() {
                    client_tick_event_writer.send(bevy_events::ClientTickEvent(tick));
                }

                // Server Tick Event
                let mut server_tick_event_writer = world_cell
                    .get_resource_mut::<Events<bevy_events::ServerTickEvent>>()
                    .unwrap();
                for tick in events.read::<naia_events::ServerTickEvent>() {
                    server_tick_event_writer.send(bevy_events::ServerTickEvent(tick));
                }

                // Message Event
                let mut message_event_writer = world_cell
                    .get_resource_mut::<Events<bevy_events::MessageEvents>>()
                    .unwrap();
                message_event_writer.send(bevy_events::MessageEvents::from(&mut events));

                // Update Component Event
                if let Some(updates) = events.world.take_updates() {
                    let mut update_component_event_writer = world_cell
                        .get_resource_mut::<Events<bevy_events::UpdateComponentEvents>>()
                        .unwrap();
                    update_component_event_writer
                        .send(bevy_events::UpdateComponentEvents::new(updates));
                }

                // Spawn, Despawn, Insert, Remove Events
                BevyWorldEvents::write_events(&mut events.world, world);
            }
        }
    });
}

pub fn should_receive(client: Res<Client<Entity>>) -> bool {
    client.is_connecting()
}
