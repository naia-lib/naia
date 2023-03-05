use bevy_ecs::{
    entity::Entity,
    event::Events,
    schedule::ShouldRun,
    system::Res,
    world::{Mut, World},
};

use naia_bevy_shared::{events::BevyWorldEvents, WorldProxyMut};
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
                // Connect Event
                let mut connect_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::ConnectEvent>>()
                    .unwrap();
                for _ in events.read::<naia_events::ConnectEvent>() {
                    connect_event_writer.send(bevy_events::ConnectEvent);
                }

                // Disconnect Event
                let mut disconnect_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::DisconnectEvent>>()
                    .unwrap();
                for _ in events.read::<naia_events::DisconnectEvent>() {
                    disconnect_event_writer.send(bevy_events::DisconnectEvent);
                }

                // Reject Event
                let mut reject_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::RejectEvent>>()
                    .unwrap();
                for _ in events.read::<naia_events::RejectEvent>() {
                    reject_event_writer.send(bevy_events::RejectEvent);
                }

                // Error Event
                let mut error_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::ErrorEvent>>()
                    .unwrap();
                for error in events.read::<naia_events::ErrorEvent>() {
                    error_event_writer.send(bevy_events::ErrorEvent(error));
                }

                // Client Tick Event
                let mut client_tick_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::ClientTickEvent>>()
                    .unwrap();
                for tick in events.read::<naia_events::ClientTickEvent>() {
                    client_tick_event_writer.send(bevy_events::ClientTickEvent(tick));
                }

                // Server Tick Event
                let mut server_tick_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::ServerTickEvent>>()
                    .unwrap();
                for tick in events.read::<naia_events::ServerTickEvent>() {
                    server_tick_event_writer.send(bevy_events::ServerTickEvent(tick));
                }

                // Message Event
                let mut message_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::MessageEvents>>()
                    .unwrap();
                message_event_writer.send(bevy_events::MessageEvents::from(&mut events));

                // Update Component Event
                if let Some(updates) = events.world.take_updates() {
                    let mut update_component_event_writer = world
                        .get_resource_unchecked_mut::<Events<bevy_events::UpdateComponentEvents>>()
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
