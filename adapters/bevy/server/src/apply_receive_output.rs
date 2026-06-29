use bevy_ecs::{
    entity::Entity,
    message::Messages,
    world::{Mut, World},
};
use naia_bevy_shared::{HostOwned, WorldProxy, WorldRefType};
use naia_server::{
    pipeline_actors::{EventReceiver, CoordHandle},
    EntityOwner, Events, ReceiveOutput,
};

use crate::{
    component_event_registry::ComponentEventRegistry, plugin::Singleton, server::ServerImpl,
    ClientOwned,
};

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
        PublishEntityEvent, RequestEvents, SpawnEntityEvent, TickEvent, UnpublishEntityEvent,
    };
}

/// Apply a [`ReceiveOutput`] to the Bevy world, firing the same events that
/// [`translate_world_events`] fires from a live [`ServerImpl`].
///
/// This is the pipeline-coordinator entry point: after the recv phase
/// completes (in 4-F this becomes a coordinator-driven step that reads the
/// recv-side world events into a `ReceiveOutput`), pass the output here
/// along with the Bevy `World` and the `ServerImpl` resource to propagate
/// all decoded events into Bevy's message queues.
///
/// # Phase 4 wiring note
///
/// 4-E.2f rewired `RecvHandle` / `SendHandle` to own their substates
/// directly (`Box<RecvState<E>>` / `Box<SendState<E>>`), and
/// `WorldServer::into_pipeline_handles` returns the three-way
/// `(CoordinatorState, RecvHandle, SendHandle)`. The bevy adapter's
/// coordinator (step 4-F) is responsible for harvesting a `ReceiveOutput`
/// from the recv-side state on each tick and calling this function with
/// it. The implementation mirrors `translate_world_events` from
/// `systems.rs` but works from a pre-collected `ReceiveOutput` instead
/// of re-locking the server.
pub fn apply_receive_output(
    world: &mut World,
    server: &mut ServerImpl,
    output: ReceiveOutput<Entity>,
) {
    // Fire one bevy `TickEvent` per server tick that the recv path advanced.
    // In Phase 4 the `translate_tick_events` system is not in the schedule,
    // so this is the sole source of `TickEvent` fan-out into Bevy.
    if !output.pending_ticks.is_empty() {
        let mut tick_writer = world
            .get_resource_mut::<Messages<bevy_events::TickEvent>>()
            .unwrap();
        for tick in output.pending_ticks {
            tick_writer.write(bevy_events::TickEvent(tick));
        }
    }

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
            if let EntityOwner::Client(user_key) = server.entity_owner(world.proxy(), &entity) {
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

    // Component / Bundle events — fan out via the registry, mirroring the
    // tail of `translate_world_events`. Without this, user-registered
    // `InsertComponentEvent<C>` / `UpdateComponentEvent<C>` /
    // `RemoveComponentEvent<C>` (and bundle equivalents) never fire under
    // `world_only_pipeline` mode, so any system reading them (e.g. the
    // level-editor cell's `sync_tile_inserts`) silently does nothing.
    world.resource_scope(|world, mut registry: Mut<ComponentEventRegistry>| {
        registry.receive_events(world, &mut events);
    });
}

/// Phase B.7 (MISSION_SIM_OWNS_WORLD) sibling of [`apply_receive_output`]
/// for the three-handle pipeline architecture.
///
/// Same event-emission body as [`apply_receive_output`], byte-for-byte,
/// EXCEPT the two `ServerImpl` query sites (`is_resource_entity`,
/// `entity_owner`) route through [`CoordHandle`] instead. The
/// `ComponentEventRegistry::receive_events` tail call is preserved
/// verbatim (lesson 11 of `feedback_naia_4f_operational`: omitting it
/// silently breaks `InsertComponentEvent<C>` / `UpdateComponentEvent<C>` /
/// `RemoveComponentEvent<C>` fan-out for any registered component type).
///
/// Cyberlith's `GameCell::update` calls this from its cross-half block
/// after `apply_recv_to_world` populates `output.world_events`.
pub fn apply_receive_output_pipeline(
    world: &mut World,
    sim_handle: &CoordHandle<Entity>,
    output: ReceiveOutput<Entity>,
) {
    // Fire one bevy `TickEvent` per server tick that the recv path advanced.
    if !output.pending_ticks.is_empty() {
        let mut tick_writer = world
            .get_resource_mut::<Messages<bevy_events::TickEvent>>()
            .unwrap();
        for tick in output.pending_ticks {
            tick_writer.write(bevy_events::TickEvent(tick));
        }
    }

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

    // Spawn Entity Event (with resource-entity filter)
    if events.has::<naia_events::SpawnEntityEvent>() {
        let mut client_spawned_entities = Vec::new();
        for (_, entity) in events.read::<naia_events::SpawnEntityEvent>() {
            if sim_handle.is_resource_entity(&entity) {
                continue;
            }
            if !world.proxy().has_entity(&entity) {
                continue;
            }
            if let EntityOwner::Client(user_key) = sim_handle.entity_owner(&entity) {
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
            if sim_handle.is_resource_entity(&entity) {
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

    // Component / Bundle events — fan out via the registry, mirroring the
    // tail of `translate_world_events`. Lesson 11 of
    // `feedback_naia_4f_operational`: omitting this tail call silently
    // breaks user-registered `InsertComponentEvent<C>` /
    // `UpdateComponentEvent<C>` / `RemoveComponentEvent<C>` even when the
    // binary packets decoded successfully.
    world.resource_scope(|world, mut registry: Mut<ComponentEventRegistry>| {
        registry.receive_events(world, &mut events);
    });
}

/// C.6-prep 7/7 sibling of [`apply_receive_output_pipeline`] that drains a
/// `ReceiveOutput<E>` ONCE and fans into BOTH sinks:
///   - the bevy `Messages<X>` buffers (the same fan-out
///     `apply_receive_output_pipeline` performs), AND
///   - a [`EventReceiver`]`<Entity>` (the same fan-out
///     [`EventReceiver::push_from_receive_output`] performs).
///
/// # Why this exists
///
/// The C.6-prep spec (§1.4) shows cyberlith Sim calling both
/// `apply_receive_output_pipeline(&output)` and
/// `sim_event_receiver.push_from_receive_output(&output)` on a shared
/// reference. The two functions both take `ReceiveOutput<E>` by value
/// and drain `output.world_events`, so they can't actually be chained
/// against the same output without double-consuming. `WorldEvents<E>`
/// is not `Clone`. This helper resolves the gap with a single drain
/// + dual sink.
///
/// # Fan-out strategy
///
/// Walks `Events<E>` event-type-by-event-type. For each type:
///   * drains the underlying vec/map ONCE via `events.read::<X>()` or
///     a one-shot `take_*` call,
///   * pushes each item into the bevy `Messages<X>` writer (same
///     behavior as `apply_receive_output_pipeline`),
///   * and pushes a clone-or-equivalent into the matching
///     `EventReceiver` typed buffer via `push_connect` /
///     `push_disconnect` / `push_spawn_client_owned` / etc.
///
/// Per-event clone costs are negligible: `UserKey` / `Entity` /
/// `DisconnectReason` / `Tick` are `Copy`; `NaiaServerError` is
/// collapsed to a `format!("{err:?}")` string (matching
/// `push_from_receive_output`'s existing behavior); `MessageContainer`
/// is Arc-internal so its `Clone` is cheap.
///
/// # Coexistence
///
/// The original [`apply_receive_output_pipeline`] and
/// [`EventReceiver::push_from_receive_output`] functions are
/// unchanged; existing single-sink callers (namako tests, cyberlith
/// pre-C.6.f, this crate's other tests) keep working byte-for-byte.
///
/// # Byte parity
///
/// The bevy `Messages<X>` fan-out produced by this helper is
/// byte-identical to what `apply_receive_output_pipeline` would
/// produce on the same input: same event ordering, same payload
/// constructions, same `ComponentEventRegistry::receive_events` tail
/// call (lesson 11 of `feedback_naia_4f_operational`).
pub fn apply_receive_output_pipeline_with_event_receiver(
    world: &mut World,
    sim_handle: &CoordHandle<Entity>,
    sim_receiver: &EventReceiver<Entity>,
    output: ReceiveOutput<Entity>,
) {
    apply_receive_output_pipeline_with_event_receiver_split(
        world,
        None,
        sim_handle,
        sim_receiver,
        output,
    )
}

/// Dual-target variant of [`apply_receive_output_pipeline_with_event_receiver`].
///
/// When the consumer hosts replicated entities on a world OTHER than the
/// coordinator world (e.g. cyberlith's editor cells, whose client-published
/// entities live on the Sim SubApp world), pass that world as `entity_world`.
/// The fan-out then splits by scope:
///
/// - **`entity_world`** receives everything entity-scoped: the
///   Spawn/Despawn/Publish/Unpublish `Messages<X>` buffers, the
///   `ClientOwned`/`HostOwned` marker mutations (Delegate / AuthGrant /
///   AuthReset arms), the `SpawnEntityEvent` `has_entity` existence check,
///   and the `ComponentEventRegistry::receive_events` tail (so
///   `InsertComponentEvent<C>` / `UpdateComponentEvent<C>` /
///   `RemoveComponentEvent<C>` fire into `entity_world`'s buffers — which
///   must therefore have the registry + Messages registered).
/// - **`world`** (the coordinator) keeps everything connection-scoped:
///   Tick / Connect / Disconnect / Error / Message / Request / Auth events.
///
/// `entity_world: None` collapses to the single-world behavior,
/// byte-identical to [`apply_receive_output_pipeline_with_event_receiver`].
/// `EventReceiver` pushes are unchanged in both modes.
pub fn apply_receive_output_pipeline_with_event_receiver_split(
    world: &mut World,
    mut entity_world: Option<&mut World>,
    sim_handle: &CoordHandle<Entity>,
    sim_receiver: &EventReceiver<Entity>,
    output: ReceiveOutput<Entity>,
) {
    // Fire one bevy `TickEvent` + push one RecvTickEvent per server
    // tick that the recv path advanced.
    if !output.pending_ticks.is_empty() {
        let mut tick_writer = world
            .get_resource_mut::<Messages<bevy_events::TickEvent>>()
            .unwrap();
        for tick in &output.pending_ticks {
            tick_writer.write(bevy_events::TickEvent(*tick));
        }
        for tick in output.pending_ticks {
            sim_receiver.push_tick(tick);
        }
    }

    let mut events: Events<Entity> = Events::from(output.world_events);

    if events.is_empty() {
        return;
    }

    // Connect Event → bevy Messages + sim receiver
    if events.has::<naia_events::ConnectEvent>() {
        let mut event_writer = world
            .get_resource_mut::<Messages<bevy_events::ConnectEvent>>()
            .unwrap();
        for user_key in events.read::<naia_events::ConnectEvent>() {
            event_writer.write(bevy_events::ConnectEvent(user_key));
            sim_receiver.push_connect(user_key);
        }
    }

    // Disconnect Event → bevy Messages + sim receiver
    if events.has::<naia_events::DisconnectEvent>() {
        let mut event_writer = world
            .get_resource_mut::<Messages<bevy_events::DisconnectEvent>>()
            .unwrap();
        for (user_key, user, reason) in events.read::<naia_events::DisconnectEvent>() {
            event_writer.write(bevy_events::DisconnectEvent(user_key, user, reason));
            sim_receiver.push_disconnect(user_key, user, reason);
        }
    }

    // Error Event → bevy Messages + sim receiver
    // (sim_receiver collapses the error to a Debug-formatted string,
    // matching `push_from_receive_output`.)
    if events.has::<naia_events::ErrorEvent>() {
        let mut event_writer = world
            .get_resource_mut::<Messages<bevy_events::ErrorEvent>>()
            .unwrap();
        for error in events.read::<naia_events::ErrorEvent>() {
            let debug = format!("{error:?}");
            event_writer.write(bevy_events::ErrorEvent(error));
            sim_receiver.push_error(debug);
        }
    }

    // Message Event → bevy MessageEvents + sim receiver.
    //
    // `events.take_messages()` drains the messages map once. We clone
    // each (channel, message) entry into the sim receiver, then move
    // the map into a `bevy_events::MessageEvents` (constructed via
    // `MessageEvents::from_inner` so the inner shape matches what
    // `MessageEvents::from(&mut events)` would have produced).
    // `MessageContainer` clone is Arc-internal so the fan-out clone
    // is cheap.
    if events.has_messages() {
        let messages_map = events.take_messages();
        for (channel_kind, by_kind) in &messages_map {
            for (message_kind, list) in by_kind {
                for (user_key, container) in list {
                    sim_receiver.push_message(
                        *channel_kind,
                        *message_kind,
                        *user_key,
                        container.clone(),
                    );
                }
            }
        }
        let mut event_writer = world
            .get_resource_mut::<Messages<bevy_events::MessageEvents>>()
            .unwrap();
        event_writer.write(bevy_events::MessageEvents::from_inner(messages_map));
    }

    // Request Event — bevy-only path. EventReceiver does not fan
    // requests out (per `push_from_receive_output`'s intentional
    // omission).
    if events.has_requests() {
        let mut event_writer = world
            .get_resource_mut::<Messages<bevy_events::RequestEvents>>()
            .unwrap();
        event_writer.write(bevy_events::RequestEvents::from(&mut events));
    }

    // Auth Event — bevy-only path.
    if events.has_auths() {
        let mut event_writer = world
            .get_resource_mut::<Messages<bevy_events::AuthEvents>>()
            .unwrap();
        event_writer.write(bevy_events::AuthEvents::from(&mut events));
    }

    // ── Entity-scoped fan-out below: target the entity world (the world
    // that `apply_recv_to_world` mutated), which is `world` itself in the
    // single-world mode and the caller-provided split target otherwise.
    let ew: &mut World = match entity_world.as_deref_mut() {
        Some(w) => w,
        None => world,
    };

    // Spawn Entity Event — apply the same resource-entity +
    // client-owned filters as the legacy path. Same suppression in
    // the sim_receiver matches `push_from_receive_output` semantics.
    if events.has::<naia_events::SpawnEntityEvent>() {
        let mut client_spawned_entities = Vec::new();
        for (_, entity) in events.read::<naia_events::SpawnEntityEvent>() {
            if sim_handle.is_resource_entity(&entity) {
                continue;
            }
            if !ew.proxy().has_entity(&entity) {
                continue;
            }
            if let EntityOwner::Client(user_key) = sim_handle.entity_owner(&entity) {
                client_spawned_entities.push((user_key, entity));
            }
        }
        {
            let mut event_writer = ew
                .get_resource_mut::<Messages<bevy_events::SpawnEntityEvent>>()
                .unwrap();
            for &(user_key, entity) in &client_spawned_entities {
                event_writer.write(bevy_events::SpawnEntityEvent(user_key, entity));
                sim_receiver.push_spawn_client_owned(user_key, entity);
            }
        }
        for (user_key, entity) in client_spawned_entities {
            ew.entity_mut(entity).insert(ClientOwned(user_key));
        }
    }

    // Despawn Entity Event — resource-entity filter; same in both
    // sinks per `push_from_receive_output`.
    if events.has::<naia_events::DespawnEntityEvent>() {
        let mut event_writer = ew
            .get_resource_mut::<Messages<bevy_events::DespawnEntityEvent>>()
            .unwrap();
        for (user_key, entity) in events.read::<naia_events::DespawnEntityEvent>() {
            if sim_handle.is_resource_entity(&entity) {
                continue;
            }
            event_writer.write(bevy_events::DespawnEntityEvent(user_key, entity));
            sim_receiver.push_despawn(user_key, entity);
        }
    }

    // Publish Entity Event
    if events.has::<naia_events::PublishEntityEvent>() {
        let mut event_writer = ew
            .get_resource_mut::<Messages<bevy_events::PublishEntityEvent>>()
            .unwrap();
        for (user_key, entity) in events.read::<naia_events::PublishEntityEvent>() {
            event_writer.write(bevy_events::PublishEntityEvent(user_key, entity));
            sim_receiver.push_publish(user_key, entity);
        }
    }

    // Unpublish Entity Event
    if events.has::<naia_events::UnpublishEntityEvent>() {
        let mut event_writer = ew
            .get_resource_mut::<Messages<bevy_events::UnpublishEntityEvent>>()
            .unwrap();
        for (user_key, entity) in events.read::<naia_events::UnpublishEntityEvent>() {
            event_writer.write(bevy_events::UnpublishEntityEvent(user_key, entity));
            sim_receiver.push_unpublish(user_key, entity);
        }
    }

    // Delegate Entity Event — bevy world mutation only
    // (EventReceiver does not surface this event type).
    if events.has::<naia_events::DelegateEntityEvent>() {
        for (_, entity) in events.read::<naia_events::DelegateEntityEvent>() {
            ew.entity_mut(entity).insert(HostOwned::new::<Singleton>());
        }
    }

    // Entity Auth Grant Event
    if events.has::<naia_events::EntityAuthGrantEvent>() {
        for (_, entity) in events.read::<naia_events::EntityAuthGrantEvent>() {
            ew.entity_mut(entity).remove::<HostOwned>();
        }
    }

    // Entity Auth Reset Event
    if events.has::<naia_events::EntityAuthResetEvent>() {
        for entity in events.read::<naia_events::EntityAuthResetEvent>() {
            if let Ok(mut entity_mut) = ew.get_entity_mut(entity) {
                entity_mut.insert(HostOwned::new::<Singleton>());
            }
        }
    }

    // Component / Bundle events — same registry call as the legacy
    // path (lesson 11 of `feedback_naia_4f_operational`). Fires into the
    // entity world's registry + Messages buffers (in split mode the entity
    // world MUST have a `ComponentEventRegistry` with the replicated
    // component kinds registered, or this `resource_scope` panics).
    ew.resource_scope(|ew, mut registry: Mut<ComponentEventRegistry>| {
        registry.receive_events(ew, &mut events);
    });
}
