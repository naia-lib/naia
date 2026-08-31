#![cfg(test)]
//! Coverage for the two sync engines -- `remote_engine.rs` and `host_engine.rs`.
//!
//! The engines are thin, but every channel test in this suite reaches its
//! subject *through* one of them, which is exactly why their own small methods
//! went unasserted: a neutered accessor or a deleted match arm still lets the
//! channel-level tests pass. A sweep showed the hand-off drains, the entity
//! registry lookups, the `send_entity_command` arms and the host receive-side
//! type gate could all be replaced with constants unnoticed.

use crate::world::local::local_entity::{HostEntity, RemoteEntity};
use crate::world::sync::RemoteEntityChannel;
use crate::{
    world::{
        component::component_kinds::ComponentKind,
        entity::in_scope_entities::InScopeEntities,
        sync::{HostEngine, HostEntityChannel, RemoteEngine},
    },
    BigMapKey, EntityAuthStatus, EntityCommand, EntityMessage, GlobalEntity, HostType,
    LocalEntityMap,
};

fn component_kind<T: 'static>() -> ComponentKind {
    ComponentKind::from(std::any::TypeId::of::<T>())
}

struct Alpha;
struct Beta;

// ===========================================================================
// RemoteEngine
// ===========================================================================

fn remote_engine() -> RemoteEngine<RemoteEntity> {
    RemoteEngine::new(HostType::Client)
}

/// Taking the incoming events is a hand-off: the caller applies them to the
/// ECS and discards them, so a second take must come back empty or every
/// spawn is applied twice.
#[test]
fn taking_incoming_events_hands_them_over() {
    let entity = RemoteEntity::new(1);
    let mut engine = remote_engine();
    engine.receive_message(1, EntityMessage::Spawn(entity));

    let first = engine.take_incoming_events();
    let second = engine.take_incoming_events();

    assert!(!first.is_empty(), "the spawn never reached the engine");
    assert!(second.is_empty(), "the take left the events behind");
}

/// The outgoing twin, driven through the one method whose whole job is to
/// queue a command directly on the engine.
#[test]
fn taking_outgoing_commands_hands_them_over() {
    let global_entity = GlobalEntity::from_u64(1);
    let mut engine = remote_engine();

    engine.push_outgoing_despawn(EntityCommand::Despawn(global_entity));

    let first = engine.take_outgoing_commands();
    let second = engine.take_outgoing_commands();

    assert_eq!(
        first,
        vec![EntityCommand::Despawn(global_entity)],
        "the queued despawn never reached the outgoing buffer",
    );
    assert!(second.is_empty(), "the take left the commands behind");
}

/// `has_entity` must consult the real registry. Both directions are asserted
/// on the same engine so neither constant answer survives.
#[test]
fn a_remote_engine_reports_only_the_entities_it_tracks() {
    let entity = RemoteEntity::new(1);
    let other = RemoteEntity::new(2);
    let mut engine = remote_engine();

    assert!(!engine.has_entity(&entity), "a fresh engine tracks nothing");

    engine.receive_message(1, EntityMessage::Spawn(entity));

    assert!(engine.has_entity(&entity));
    assert!(
        !engine.has_entity(&other),
        "one spawn registered every entity",
    );
}

/// `InScopeEntities` is a separate impl over the same map -- the scope checks
/// in the world managers go through it, so it needs its own both-directions
/// assertion.
#[test]
fn the_in_scope_view_of_a_remote_engine_follows_the_registry() {
    let entity = RemoteEntity::new(1);
    let mut engine = remote_engine();

    assert!(
        !InScopeEntities::has_entity(&engine, &entity),
        "a fresh engine has nothing in scope",
    );

    engine.receive_message(1, EntityMessage::Spawn(entity));

    assert!(InScopeEntities::has_entity(&engine, &entity));
    assert!(
        !InScopeEntities::has_entity(&engine, &RemoteEntity::new(2)),
        "one spawn brought every entity into scope",
    );
}

/// The channel lookup has to find the tracked channel and refuse the untracked
/// one -- a constant `None` strands every migration path that reaches for a
/// channel, and a lookup that ignores its key hands back the wrong channel.
#[test]
fn the_channel_lookup_finds_only_tracked_entities() {
    let entity = RemoteEntity::new(1);
    let mut engine = remote_engine();
    engine.receive_message(1, EntityMessage::Spawn(entity));

    assert!(
        engine.get_entity_channel_mut(&entity).is_some(),
        "the spawned entity's channel was not found",
    );
    assert!(
        engine
            .get_entity_channel_mut(&RemoteEntity::new(2))
            .is_none(),
        "an untracked entity produced a channel",
    );
}

/// The mutable world view is the real map, not a copy: a removal made through
/// it has to be visible to the engine afterwards.
#[test]
fn the_mutable_world_view_is_the_engines_own_registry() {
    let entity = RemoteEntity::new(1);
    let mut engine = remote_engine();
    engine.receive_message(1, EntityMessage::Spawn(entity));

    engine.get_world_mut().remove(&entity);

    assert!(
        !engine.has_entity(&entity),
        "the mutable view handed back a copy",
    );
}

/// A migrated channel never receives the `Spawn` whose arm normally drains it,
/// so the engine needs this explicit flush -- without it the released messages
/// only surface when the next message for the entity happens to arrive.
#[test]
fn flushing_a_channel_moves_its_ready_messages_up_to_the_engine() {
    let entity = RemoteEntity::new(1);
    let mut engine = remote_engine();

    // Parks behind the spawn barrier: the channel is still Despawned.
    engine.receive_message(3, EntityMessage::Despawn(entity));
    assert!(
        engine.take_incoming_events().is_empty(),
        "fixture: the despawn should still be buffered",
    );

    engine
        .get_entity_channel_mut(&entity)
        .expect("fixture: the channel should have been created lazily")
        .force_drain_all_buffers();
    assert!(
        engine.take_incoming_events().is_empty(),
        "fixture: force-draining a channel must not reach the engine by itself",
    );

    engine.flush_entity_channel(entity);

    assert!(
        !engine.take_incoming_events().is_empty(),
        "the flush left the channel's ready messages behind",
    );
}

/// Flushing an entity the engine does not track is a no-op, not a panic --
/// migration teardown races call it for entities already removed.
#[test]
fn flushing_an_unknown_entity_is_a_no_op() {
    let mut engine = remote_engine();

    engine.flush_entity_channel(RemoteEntity::new(7));

    assert!(engine.take_incoming_events().is_empty());
}

/// A `Noop` carries no entity; taking one seriously would panic on
/// `entity().unwrap()`, so the early return has to hold.
#[test]
fn a_noop_message_is_dropped_without_creating_a_channel() {
    let mut engine = remote_engine();

    engine.receive_message(1, EntityMessage::Noop);

    assert!(engine.take_incoming_events().is_empty());
    assert!(
        engine.get_world().is_empty(),
        "a Noop created an entity channel",
    );
}

/// The auth command has to reach the channel *and* be drained out to the
/// engine's outgoing buffer, or every authority request the client raises is
/// silently stranded.
#[test]
fn an_auth_command_reaches_the_engines_outgoing_buffer() {
    let entity = RemoteEntity::new(1);
    let global_entity = GlobalEntity::from_u64(1);
    let mut engine = remote_engine();
    engine.insert_entity_channel(entity, RemoteEntityChannel::new_delegated(HostType::Client));

    engine.send_auth_command(
        entity,
        EntityCommand::RequestAuthority(Some(0), global_entity),
    );

    assert!(
        !engine.take_outgoing_commands().is_empty(),
        "the auth command never left the channel",
    );
}

/// The guard is the other half: sending to an untracked entity is a caller
/// bug, and inverting it would send commands into a channel that does not
/// exist while rejecting every legitimate one.
#[test]
#[should_panic(expected = "Cannot send a command to an entity that does not exist")]
fn an_auth_command_for_an_unknown_entity_is_rejected() {
    let mut engine = remote_engine();

    engine.send_auth_command(
        RemoteEntity::new(1),
        EntityCommand::RequestAuthority(Some(0), GlobalEntity::from_u64(1)),
    );
}

#[test]
#[should_panic(expected = "Cannot send a command to an entity that does not exist")]
fn an_entity_command_for_an_unknown_entity_is_rejected() {
    let mut engine = remote_engine();

    engine.send_entity_command(
        RemoteEntity::new(1),
        EntityCommand::Despawn(GlobalEntity::from_u64(1)),
    );
}

/// `send_entity_command` is local teardown: a `Despawn` drops the channel
/// outright and must NOT queue anything for the wire (that is
/// `push_outgoing_despawn`'s separate, intentional job).
#[test]
fn a_local_despawn_command_drops_the_entity_channel() {
    let entity = RemoteEntity::new(1);
    let mut engine = remote_engine();
    engine.receive_message(1, EntityMessage::Spawn(entity));

    engine.send_entity_command(entity, EntityCommand::Despawn(GlobalEntity::from_u64(1)));

    assert!(
        !engine.has_entity(&entity),
        "the despawned entity's channel outlived it",
    );
    assert!(
        engine.take_outgoing_commands().is_empty(),
        "a local teardown despawn was queued for the wire",
    );
}

/// The component arms maintain the channel's kind registry, which is what the
/// world managers consult before applying an insert or remove.
#[test]
fn the_component_command_arms_maintain_the_channels_kind_registry() {
    let entity = RemoteEntity::new(1);
    let global_entity = GlobalEntity::from_u64(1);
    let mut engine = remote_engine();
    engine.receive_message(1, EntityMessage::Spawn(entity));

    engine.send_entity_command(
        entity,
        EntityCommand::InsertComponent(global_entity, component_kind::<Alpha>()),
    );
    engine.send_entity_command(
        entity,
        EntityCommand::InsertComponent(global_entity, component_kind::<Beta>()),
    );

    {
        let channel = engine.get_entity_channel_mut(&entity).unwrap();
        assert!(channel.has_component_kind(&component_kind::<Alpha>()));
        assert!(channel.has_component_kind(&component_kind::<Beta>()));
    }

    engine.send_entity_command(
        entity,
        EntityCommand::RemoveComponent(global_entity, component_kind::<Alpha>()),
    );

    let channel = engine.get_entity_channel_mut(&entity).unwrap();
    assert!(
        !channel.has_component_kind(&component_kind::<Alpha>()),
        "the remove arm did not unregister the kind",
    );
    assert!(
        channel.has_component_kind(&component_kind::<Beta>()),
        "the remove arm cleared kinds it was not given",
    );
}

/// Commands that belong to the auth system fall through the match untouched --
/// they must not disturb the channel the way the handled arms do.
#[test]
fn an_auth_shaped_command_falls_through_send_entity_command() {
    let entity = RemoteEntity::new(1);
    let mut engine = remote_engine();
    engine.receive_message(1, EntityMessage::Spawn(entity));

    engine.send_entity_command(
        entity,
        EntityCommand::Publish(Some(0), GlobalEntity::from_u64(1)),
    );

    assert!(
        engine.has_entity(&entity),
        "a fall-through command tore down the channel",
    );
    assert!(engine.take_outgoing_commands().is_empty());
}

/// The auth status is read back out of the channel, not cached on the engine:
/// an update has to be visible through the getter, and an untracked entity has
/// no status at all.
#[test]
fn the_engine_reports_the_channels_live_auth_status() {
    let entity = RemoteEntity::new(1);
    let mut engine = remote_engine();
    engine.insert_entity_channel(entity, RemoteEntityChannel::new_delegated(HostType::Client));

    assert_ne!(
        engine.get_entity_auth_status(&entity),
        Some(EntityAuthStatus::Granted),
        "fixture: the channel must not start out Granted",
    );

    engine.receive_set_auth_status(entity, EntityAuthStatus::Granted);

    assert_eq!(
        engine.get_entity_auth_status(&entity),
        Some(EntityAuthStatus::Granted),
        "the status update never reached the channel",
    );
    assert_eq!(
        engine.get_entity_auth_status(&RemoteEntity::new(2)),
        None,
        "an untracked entity reported an auth status",
    );
}

/// Setting the status of an entity the engine does not track is a no-op rather
/// than a panic -- late migration acks arrive for entities already gone.
#[test]
fn setting_the_auth_status_of_an_unknown_entity_is_a_no_op() {
    let mut engine = remote_engine();

    engine.receive_set_auth_status(RemoteEntity::new(1), EntityAuthStatus::Granted);

    assert!(engine.get_world().is_empty());
}

/// The migration path lifts a channel out of one engine and inserts it into
/// another; both halves have to move the real channel.
#[test]
fn a_channel_can_be_lifted_out_of_one_engine_and_into_another() {
    let entity = RemoteEntity::new(1);
    let mut source = remote_engine();
    let mut destination = remote_engine();
    source.insert_entity_channel(entity, RemoteEntityChannel::new_delegated(HostType::Client));
    source.receive_set_auth_status(entity, EntityAuthStatus::Granted);

    let channel = source.remove_entity_channel(&entity);

    assert!(
        !source.has_entity(&entity),
        "the channel was copied, not removed",
    );

    destination.insert_entity_channel(entity, channel);

    assert_eq!(
        destination.get_entity_auth_status(&entity),
        Some(EntityAuthStatus::Granted),
        "the moved channel lost its state on the way",
    );
}

#[test]
#[should_panic(expected = "Cannot remove entity channel that doesn't exist")]
fn removing_an_untracked_channel_panics() {
    let mut engine = remote_engine();

    engine.remove_entity_channel(&RemoteEntity::new(1));
}

#[test]
#[should_panic(expected = "Cannot insert entity channel that already exists")]
fn inserting_over_a_tracked_channel_panics() {
    let entity = RemoteEntity::new(1);
    let mut engine = remote_engine();
    engine.insert_entity_channel(entity, RemoteEntityChannel::new(HostType::Client));

    engine.insert_entity_channel(entity, RemoteEntityChannel::new(HostType::Client));
}

// ===========================================================================
// HostEngine
// ===========================================================================

/// A host engine plus a converter that maps `GlobalEntity(n)` to
/// `HostEntity(n)` for the entities the test names.
fn host_engine(entity_ids: &[u32]) -> (HostEngine, LocalEntityMap) {
    let mut map = LocalEntityMap::new(HostType::Server);
    for id in entity_ids {
        map.insert_with_host_entity(GlobalEntity::from_u64(*id as u64), HostEntity::new(*id));
    }
    (HostEngine::new(HostType::Server), map)
}

fn spawn_on_host(engine: &mut HostEngine, map: &LocalEntityMap, id: u32) {
    engine.send_command(
        map.entity_converter(),
        EntityCommand::Spawn(GlobalEntity::from_u64(id as u64)),
    );
    let _ = engine.take_outgoing_commands();
}

/// The host `Spawn` arm both registers the entity channel and queues the
/// command for the wire; losing either half strands the entity.
#[test]
fn a_host_spawn_registers_the_channel_and_queues_the_command() {
    let (mut engine, map) = host_engine(&[1]);

    engine.send_command(
        map.entity_converter(),
        EntityCommand::Spawn(GlobalEntity::from_u64(1)),
    );

    assert!(
        engine.get_entity_channel(&HostEntity::new(1)).is_some(),
        "the spawn did not register an entity channel",
    );
    assert_eq!(
        engine.take_outgoing_commands(),
        vec![EntityCommand::Spawn(GlobalEntity::from_u64(1))],
        "the spawn was never queued for the wire",
    );
}

/// The coalesced twin additionally seeds the channel's component kinds.
#[test]
fn a_host_coalesced_spawn_seeds_the_channels_component_kinds() {
    let (mut engine, map) = host_engine(&[1]);

    engine.send_command(
        map.entity_converter(),
        EntityCommand::SpawnWithComponents(
            GlobalEntity::from_u64(1),
            vec![component_kind::<Alpha>()],
        ),
    );

    let channel = engine
        .get_entity_channel(&HostEntity::new(1))
        .expect("the coalesced spawn did not register an entity channel");
    assert!(
        channel
            .component_kinds()
            .contains(&component_kind::<Alpha>()),
        "the coalesced components were not seeded",
    );
    assert!(
        !channel
            .component_kinds()
            .contains(&component_kind::<Beta>()),
        "the seed registered kinds it was not given",
    );
    assert!(!engine.take_outgoing_commands().is_empty());
}

#[test]
#[should_panic(expected = "Cannot spawn an entity that already exists")]
fn spawning_an_already_tracked_host_entity_panics() {
    let (mut engine, map) = host_engine(&[1]);
    spawn_on_host(&mut engine, &map, 1);

    engine.send_command(
        map.entity_converter(),
        EntityCommand::Spawn(GlobalEntity::from_u64(1)),
    );
}

/// The host `Despawn` arm drops the channel and queues the command.
#[test]
fn a_host_despawn_drops_the_channel_and_queues_the_command() {
    let (mut engine, map) = host_engine(&[1]);
    spawn_on_host(&mut engine, &map, 1);

    engine.send_command(
        map.entity_converter(),
        EntityCommand::Despawn(GlobalEntity::from_u64(1)),
    );

    assert!(
        engine.get_entity_channel(&HostEntity::new(1)).is_none(),
        "the despawned entity's channel outlived it",
    );
    assert_eq!(
        engine.take_outgoing_commands(),
        vec![EntityCommand::Despawn(GlobalEntity::from_u64(1))],
    );
}

#[test]
#[should_panic(expected = "Cannot despawn an entity that does not exist")]
fn despawning_an_untracked_host_entity_panics() {
    let (mut engine, map) = host_engine(&[1]);

    engine.send_command(
        map.entity_converter(),
        EntityCommand::Despawn(GlobalEntity::from_u64(1)),
    );
}

/// Everything that is not a lifecycle command routes through the entity
/// channel and is drained out of it.
#[test]
fn a_host_auth_command_routes_through_the_entity_channel() {
    let (mut engine, map) = host_engine(&[1]);
    spawn_on_host(&mut engine, &map, 1);

    engine.send_command(
        map.entity_converter(),
        EntityCommand::EnableDelegation(Some(0), GlobalEntity::from_u64(1)),
    );

    assert!(
        !engine.take_outgoing_commands().is_empty(),
        "the auth command never left the entity channel",
    );
}

#[test]
#[should_panic(expected = "Cannot accept command for an entity that does not exist")]
fn a_host_command_for_an_untracked_entity_panics() {
    let (mut engine, map) = host_engine(&[1]);

    engine.send_command(
        map.entity_converter(),
        EntityCommand::EnableDelegation(Some(0), GlobalEntity::from_u64(1)),
    );
}

/// The reservation has to reach the engine's outgoing buffer on its own,
/// without waiting for a subsequent `send_command` to drain the channel.
#[test]
fn a_reserved_first_command_reaches_the_outgoing_buffer_immediately() {
    let (mut engine, map) = host_engine(&[1]);
    spawn_on_host(&mut engine, &map, 1);

    engine.reserve_first_command(
        map.entity_converter(),
        EntityCommand::EnableDelegation(Some(0), GlobalEntity::from_u64(1)),
    );

    assert!(
        !engine.take_outgoing_commands().is_empty(),
        "the reserved command was stranded in the channel",
    );
}

#[test]
#[should_panic(expected = "no entity channel for host_entity")]
fn reserving_on_an_untracked_host_entity_panics() {
    let (mut engine, map) = host_engine(&[1]);

    engine.reserve_first_command(
        map.entity_converter(),
        EntityCommand::EnableDelegation(Some(0), GlobalEntity::from_u64(1)),
    );
}

/// A client holding authority may despawn a server-created entity. The host
/// side drops the channel and surfaces the event.
#[test]
fn a_received_host_despawn_drops_the_channel_and_surfaces_the_event() {
    let (mut engine, map) = host_engine(&[1]);
    spawn_on_host(&mut engine, &map, 1);
    let host_entity = HostEntity::new(1);

    engine.receive_message(1, EntityMessage::Despawn(host_entity));

    assert_eq!(
        engine.take_incoming_events(),
        vec![EntityMessage::Despawn(host_entity)],
        "the despawn was not surfaced",
    );
    assert!(
        engine.get_entity_channel(&host_entity).is_none(),
        "the despawned entity's channel outlived it",
    );
}

/// A despawn for an entity the host no longer tracks is discarded, not
/// surfaced -- a stale packet must not produce a second despawn event.
#[test]
fn a_received_despawn_for_an_unknown_host_entity_is_discarded() {
    let (mut engine, _map) = host_engine(&[1]);

    engine.receive_message(1, EntityMessage::Despawn(HostEntity::new(1)));

    assert!(engine.take_incoming_events().is_empty());
}

/// The host never accepts world-shaping messages -- accepting them would let a
/// client spawn entities in the server's own host world.
#[test]
#[should_panic(expected = "Host should not receive messages of this type")]
fn the_host_refuses_a_received_spawn() {
    let (mut engine, _map) = host_engine(&[1]);

    engine.receive_message(1, EntityMessage::Spawn(HostEntity::new(1)));
}

#[test]
#[should_panic(expected = "Host should not receive messages of this type")]
fn the_host_refuses_a_received_component_insert() {
    let (mut engine, _map) = host_engine(&[1]);

    engine.receive_message(
        1,
        EntityMessage::InsertComponent(HostEntity::new(1), component_kind::<Alpha>()),
    );
}

/// A received `Noop` carries no entity, so it has to be dropped before the
/// `entity().unwrap()` below it.
#[test]
fn a_received_host_noop_is_dropped() {
    let (mut engine, _map) = host_engine(&[1]);

    engine.receive_message(1, EntityMessage::Noop);

    assert!(engine.take_incoming_events().is_empty());
}

/// An auth message for an entity the host no longer tracks is discarded rather
/// than panicking -- reordered packets from a lagging client hit this.
#[test]
fn a_received_message_for_an_unknown_host_entity_is_discarded() {
    let (mut engine, _map) = host_engine(&[1]);

    engine.receive_message(1, EntityMessage::ReleaseAuthority(0, HostEntity::new(1)));

    assert!(engine.take_incoming_events().is_empty());
}

/// The host incoming/outgoing takes are hand-offs too.
#[test]
fn the_host_takes_hand_over_their_buffers() {
    let (mut engine, map) = host_engine(&[1]);
    spawn_on_host(&mut engine, &map, 1);
    engine.receive_message(1, EntityMessage::Despawn(HostEntity::new(1)));

    assert!(!engine.take_incoming_events().is_empty());
    assert!(
        engine.take_incoming_events().is_empty(),
        "the take left the events behind",
    );

    let (mut engine, map) = host_engine(&[1]);
    engine.send_command(
        map.entity_converter(),
        EntityCommand::Spawn(GlobalEntity::from_u64(1)),
    );

    assert!(!engine.take_outgoing_commands().is_empty());
    assert!(
        engine.take_outgoing_commands().is_empty(),
        "the take left the commands behind",
    );
}

/// Extracting an entity's queued commands empties the channel, and an
/// untracked entity yields nothing rather than panicking.
#[test]
fn extracting_a_host_entitys_commands_empties_its_channel() {
    let (mut engine, map) = host_engine(&[1]);
    spawn_on_host(&mut engine, &map, 1);
    let host_entity = HostEntity::new(1);
    engine
        .get_entity_channel_mut(&host_entity)
        .expect("fixture: the spawned entity should have a channel")
        .send_command(EntityCommand::EnableDelegation(
            Some(0),
            GlobalEntity::from_u64(1),
        ));

    let first = engine.extract_entity_commands(&host_entity);
    let second = engine.extract_entity_commands(&host_entity);

    assert!(!first.is_empty(), "the queued command was not extracted");
    assert!(second.is_empty(), "the extract left the command behind");
    assert!(
        engine
            .extract_entity_commands(&HostEntity::new(2))
            .is_empty(),
        "an untracked entity produced commands",
    );
}

/// The host migration path lifts a channel out and puts one back.
#[test]
fn a_host_channel_can_be_lifted_out_and_reinserted() {
    let (mut engine, map) = host_engine(&[1]);
    spawn_on_host(&mut engine, &map, 1);
    let host_entity = HostEntity::new(1);

    let channel = engine.remove_entity_channel(&host_entity);

    assert!(
        engine.get_entity_channel(&host_entity).is_none(),
        "the channel was copied, not removed",
    );

    engine.insert_entity_channel(host_entity, channel);

    assert!(
        engine.get_entity_channel(&host_entity).is_some(),
        "the reinserted channel was not registered",
    );
}

#[test]
#[should_panic(expected = "Cannot remove entity channel that doesn't exist")]
fn removing_an_untracked_host_channel_panics() {
    let (mut engine, _map) = host_engine(&[1]);

    engine.remove_entity_channel(&HostEntity::new(1));
}

#[test]
#[should_panic(expected = "Cannot insert entity channel that already exists")]
fn inserting_over_a_tracked_host_channel_panics() {
    let (mut engine, map) = host_engine(&[1]);
    spawn_on_host(&mut engine, &map, 1);

    engine.insert_entity_channel(HostEntity::new(1), HostEntityChannel::new(HostType::Server));
}

/// The host channel lookup has to consult the registry in both directions.
#[test]
fn the_host_channel_lookups_find_only_tracked_entities() {
    let (mut engine, map) = host_engine(&[1]);
    spawn_on_host(&mut engine, &map, 1);

    assert!(engine.get_entity_channel(&HostEntity::new(1)).is_some());
    assert!(engine.get_entity_channel(&HostEntity::new(2)).is_none());
    assert!(engine.get_entity_channel_mut(&HostEntity::new(1)).is_some());
    assert!(engine.get_entity_channel_mut(&HostEntity::new(2)).is_none());
}
