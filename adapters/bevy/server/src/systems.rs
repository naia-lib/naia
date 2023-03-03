use bevy_ecs::{
    entity::Entity,
    event::Events,
    schedule::ShouldRun,
    system::Res,
    world::{Mut, World},
};

use naia_server::Server;

use naia_bevy_shared::WorldProxyMut;

mod naia_events {
    pub use naia_server::{
        AuthEvent, ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent,
        InsertComponentEvent, MessageEvent, RemoveComponentEvent, SpawnEntityEvent, TickEvent,
        UpdateComponentEvent,
    };
}

mod bevy_events {
    pub use crate::events::{
        AuthEvents, ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent,
        InsertComponentEvents, MessageEvents, RemoveComponentEvents, SpawnEntityEvent, TickEvent,
        UpdateComponentEvents,
    };
}

pub fn before_receive_events(world: &mut World) {
    world.resource_scope(|world, mut server: Mut<Server<Entity>>| {
        let mut events = server.receive(world.proxy_mut());
        if !events.is_empty() {
            unsafe {
                // Connect Event
                let mut connect_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::ConnectEvent>>()
                    .unwrap();
                for user_key in events.read::<naia_events::ConnectEvent>() {
                    connect_event_writer.send(bevy_events::ConnectEvent(user_key));
                }

                // Disconnect Event
                let mut disconnect_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::DisconnectEvent>>()
                    .unwrap();
                for (user_key, user) in events.read::<naia_events::DisconnectEvent>() {
                    disconnect_event_writer.send(bevy_events::DisconnectEvent(user_key, user));
                }

                // Error Event
                let mut error_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::ErrorEvent>>()
                    .unwrap();
                for error in events.read::<naia_events::ErrorEvent>() {
                    error_event_writer.send(bevy_events::ErrorEvent(error));
                }

                // Tick Event
                let mut tick_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::TickEvent>>()
                    .unwrap();
                for tick in events.read::<naia_events::TickEvent>() {
                    tick_event_writer.send(bevy_events::TickEvent(tick));
                }

                // Message Event
                let mut message_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::MessageEvents>>()
                    .unwrap();
                message_event_writer.send(bevy_events::MessageEvents::from(&mut events));

                // Auth Event
                let mut auth_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::AuthEvents>>()
                    .unwrap();
                auth_event_writer.send(bevy_events::AuthEvents::from(&mut events));

                // Spawn Entity Event
                let mut spawn_entity_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::SpawnEntityEvent>>()
                    .unwrap();
                for entity in events.read::<naia_events::SpawnEntityEvent>() {
                    spawn_entity_event_writer.send(bevy_events::SpawnEntityEvent(entity));
                }

                // Despawn Entity Event
                let mut despawn_entity_event_writer = world
                    .get_resource_unchecked_mut::<Events<bevy_events::DespawnEntityEvent>>()
                    .unwrap();
                for entity in events.read::<naia_events::DespawnEntityEvent>() {
                    despawn_entity_event_writer.send(bevy_events::DespawnEntityEvent(entity));
                }

                // Insert Component Event
                if let Some(inserts) = events.world.take_inserts() {
                    let mut insert_component_event_writer = world
                        .get_resource_unchecked_mut::<Events<bevy_events::InsertComponentEvents>>()
                        .unwrap();
                    insert_component_event_writer
                        .send(bevy_events::InsertComponentEvents::new(inserts));
                }

                // Update Component Event
                if let Some(updates) = events.world.take_updates() {
                    let mut update_component_event_writer = world
                        .get_resource_unchecked_mut::<Events<bevy_events::UpdateComponentEvents>>()
                        .unwrap();
                    update_component_event_writer
                        .send(bevy_events::UpdateComponentEvents::new(updates));
                }

                // Remove Component Event
                if let Some(removes) = events.world.take_removes() {
                    let mut remove_component_event_writer = world
                        .get_resource_unchecked_mut::<Events<bevy_events::RemoveComponentEvents>>()
                        .unwrap();

                    remove_component_event_writer
                        .send(bevy_events::RemoveComponentEvents::new(removes));
                }
            }
        }
    });
}

pub fn should_receive(server: Res<Server<Entity>>) -> ShouldRun {
    if server.is_listening() {
        ShouldRun::Yes
    } else {
        ShouldRun::No
    }
}
