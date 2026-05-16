use bevy_ecs::{entity::Entity, message::Messages, world::World};
use naia_bevy_shared::{HostOwned, WorldProxy, WorldRefType};
use naia_server::{EntityOwner, Events, ReceiveOutput};

use crate::{plugin::Singleton, server::ServerImpl, ClientOwned};

mod naia_events {
    pub use naia_server::{
        ConnectEvent, DelegateEntityEvent, DespawnEntityEvent, DisconnectEvent,
        EntityAuthGrantEvent, EntityAuthResetEvent, ErrorEvent, PublishEntityEvent,
        SpawnEntityEvent, UnpublishEntityEvent,
    };
}

mod bevy_events {
    pub use crate::events::{
        AuthEvents, ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent, MessageEvents,
        PublishEntityEvent, RequestEvents, SpawnEntityEvent, UnpublishEntityEvent,
    };
}

/// Apply a [`ReceiveOutput`] to the Bevy world, firing the same events that
/// [`translate_world_events`] fires from a live [`ServerImpl`].
///
/// This is the pipeline-coordinator entry point: after `RecvHandle::receive()`
/// returns a `ReceiveOutput`, pass it here (along with the Bevy `World` and the
/// `ServerImpl` resource) to propagate all decoded events into Bevy's message
/// queues.
///
/// # Phase 3 note
///
/// The implementation mirrors `translate_world_events` from `systems.rs` but
/// works from a pre-collected `ReceiveOutput` instead of re-locking the server.
/// Defined here as a building block; the pipeline coordinator is wired up in
/// Phase 4.
#[allow(dead_code)] // Phase 3 building block — wired up in Phase 4 pipeline coordinator
pub(crate) fn apply_receive_output(
    world: &mut World,
    server: &mut ServerImpl,
    output: ReceiveOutput<Entity>,
) {
    // Convert WorldEvents<Entity> → Events<Entity> (which has all the
    // has_messages / has_requests / has_auths helpers and the From impls used
    // by the bevy event types).
    let mut events: Events<Entity> = Events::from(output.world_events);

    if events.is_empty() {
        return;
    }

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
        for (user_key, user, reason) in events.read::<naia_events::DisconnectEvent>() {
            event_writer.write(bevy_events::DisconnectEvent(user_key, user, reason));
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

    // Spawn Entity Event (with resource-entity filter, same as translate_world_events)
    if events.has::<naia_events::SpawnEntityEvent>() {
        let mut client_spawned_entities = Vec::new();
        for (_, entity) in events.read::<naia_events::SpawnEntityEvent>() {
            if server.is_resource_entity(&entity) {
                continue;
            }
            if !world.proxy().has_entity(&entity) {
                continue;
            }
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

    // Despawn Entity Event (resource-entity filter)
    if events.has::<naia_events::DespawnEntityEvent>() {
        let mut event_writer = world
            .get_resource_mut::<Messages<bevy_events::DespawnEntityEvent>>()
            .unwrap();
        for (user_key, entity) in events.read::<naia_events::DespawnEntityEvent>() {
            if server.is_resource_entity(&entity) {
                continue;
            }
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

    // Entity Auth Grant Event
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
}
