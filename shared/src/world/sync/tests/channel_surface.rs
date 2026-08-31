#![cfg(test)]
//! Coverage for the entity-channel surface that the first sweep of
//! `remote_entity_channel.rs` / `host_entity_channel.rs` showed was untested.
//!
//! These are the small methods the engines call constantly and that no test
//! ever asserted on directly: the outgoing-command drains, the component-kind
//! registry, the force-drain escape hatch, and the migration release. A sweep
//! showed each could be replaced with `()` (or made to return a constant)
//! without a single test noticing.

use crate::world::local::local_entity::RemoteEntity;
use crate::{
    world::{
        component::component_kinds::ComponentKind,
        sync::{HostEntityChannel, RemoteEntityChannel},
    },
    BigMapKey, EntityCommand, EntityMessage, GlobalEntity, HostType,
};

fn component_kind<T: 'static>() -> ComponentKind {
    ComponentKind::from(std::any::TypeId::of::<T>())
}

struct Alpha;
struct Beta;

// ---------------------------------------------------------------------------
// Component-kind registry on a remote channel.
// ---------------------------------------------------------------------------

/// `has_component_kind` must actually consult the registry. Both directions are
/// asserted against the same channel so that a constant `true` or a constant
/// `false` fails one of them.
#[test]
fn a_remote_channel_reports_only_the_component_kinds_it_holds() {
    let mut channel = RemoteEntityChannel::new(HostType::Client);
    assert!(
        !channel.has_component_kind(&component_kind::<Alpha>()),
        "a fresh channel holds no component kinds",
    );

    channel.insert_component(component_kind::<Alpha>());

    assert!(channel.has_component_kind(&component_kind::<Alpha>()));
    assert!(
        !channel.has_component_kind(&component_kind::<Beta>()),
        "inserting one kind must not register every kind",
    );
}

#[test]
fn removing_a_component_kind_unregisters_it() {
    let mut channel = RemoteEntityChannel::new(HostType::Client);
    channel.insert_component(component_kind::<Alpha>());
    channel.insert_component(component_kind::<Beta>());

    channel.remove_component(component_kind::<Alpha>());

    assert!(!channel.has_component_kind(&component_kind::<Alpha>()));
    assert!(
        channel.has_component_kind(&component_kind::<Beta>()),
        "removing one kind must not clear the rest",
    );
}

#[test]
fn inserting_a_component_kind_twice_is_idempotent() {
    let mut channel = RemoteEntityChannel::new(HostType::Client);
    channel.insert_component(component_kind::<Alpha>());
    channel.insert_component(component_kind::<Alpha>());

    channel.remove_component(component_kind::<Alpha>());

    assert!(
        !channel.has_component_kind(&component_kind::<Alpha>()),
        "the second insert created a second channel that survived the remove",
    );
}

// ---------------------------------------------------------------------------
// Outgoing command drains.
// ---------------------------------------------------------------------------

/// A command sent on a remote channel has to reach the outgoing queue, and the
/// drain has to hand it over. Both `send_command` and
/// `drain_outgoing_messages_into` could be neutered to `()` unnoticed, and
/// either failure silently strands every authority command the client raises.
#[test]
fn a_command_sent_on_a_remote_channel_is_drained_out() {
    let global_entity = GlobalEntity::from_u64(1);
    let mut channel = RemoteEntityChannel::new_delegated(HostType::Client);

    channel.send_command(EntityCommand::RequestAuthority(Some(0), global_entity));

    let mut outgoing = Vec::new();
    channel.drain_outgoing_messages_into(&mut outgoing);

    assert!(
        !outgoing.is_empty(),
        "the command never reached the outgoing queue",
    );
}

/// Draining is a hand-off, not a copy: a second drain must come back empty or
/// every command is sent twice.
#[test]
fn draining_a_remote_channel_twice_does_not_repeat_commands() {
    let global_entity = GlobalEntity::from_u64(1);
    let mut channel = RemoteEntityChannel::new_delegated(HostType::Client);
    channel.send_command(EntityCommand::RequestAuthority(Some(0), global_entity));

    let mut first = Vec::new();
    channel.drain_outgoing_messages_into(&mut first);
    let mut second = Vec::new();
    channel.drain_outgoing_messages_into(&mut second);

    assert!(!first.is_empty(), "fixture: nothing was queued to drain");
    assert!(second.is_empty(), "the drain left the commands behind");
}

/// The host-channel twin. Its drain additionally flushes a reserved-first
/// command, so a neutered drain strands the reservation too.
#[test]
fn a_command_sent_on_a_host_channel_is_drained_out() {
    let global_entity = GlobalEntity::from_u64(1);
    let mut channel = HostEntityChannel::new(HostType::Server);

    channel.send_command(EntityCommand::EnableDelegation(Some(0), global_entity));

    let mut outgoing = Vec::new();
    channel.drain_outgoing_messages_into(&mut outgoing);

    assert!(
        !outgoing.is_empty(),
        "the command never reached the outgoing queue",
    );
}

#[test]
fn draining_a_host_channel_twice_does_not_repeat_commands() {
    let global_entity = GlobalEntity::from_u64(1);
    let mut channel = HostEntityChannel::new(HostType::Server);
    channel.send_command(EntityCommand::EnableDelegation(Some(0), global_entity));

    let mut first = Vec::new();
    channel.drain_outgoing_messages_into(&mut first);
    let mut second = Vec::new();
    channel.drain_outgoing_messages_into(&mut second);

    assert!(!first.is_empty(), "fixture: nothing was queued to drain");
    assert!(second.is_empty(), "the drain left the commands behind");
}

// ---------------------------------------------------------------------------
// Host-channel delegation state.
// ---------------------------------------------------------------------------

/// `is_delegated` must reflect the sub-channel rather than a constant. The
/// before/after pair is what rules out both constant answers.
#[test]
fn a_host_channel_reports_delegation_only_once_delegated() {
    let mut channel = HostEntityChannel::new(HostType::Server);
    assert!(
        !channel.is_delegated(),
        "a fresh host channel is not delegated",
    );

    channel.local_enable_delegation();

    assert!(channel.is_delegated(), "delegation was not reflected");
}

// ---------------------------------------------------------------------------
// Force-drain and migration release.
// ---------------------------------------------------------------------------

/// `force_drain_all_buffers` is the teardown escape hatch: whatever is still
/// parked in the entity-level buffer has to come out, even though the channel
/// never spawned and the normal path would hold it forever.
///
/// A `Despawn` for an entity whose `Spawn` never arrived is exactly such a
/// message -- its arm requires `Spawned` and otherwise breaks, leaving it at
/// the front of the buffer indefinitely.
#[test]
fn force_draining_releases_messages_the_spawn_barrier_was_holding() {
    let entity = RemoteEntity::new(1);
    let mut channel = RemoteEntityChannel::new(HostType::Client);

    // The channel is still Despawned, so this parks in the entity-level buffer.
    channel.receive_message(3, EntityMessage::Despawn(()));
    let mut before = Vec::new();
    channel.drain_incoming_messages_into(entity, &mut before);
    assert!(
        before.is_empty(),
        "fixture: the message should still be buffered behind the spawn barrier",
    );

    channel.force_drain_all_buffers();

    let mut after = Vec::new();
    channel.drain_incoming_messages_into(entity, &mut after);
    assert!(
        !after.is_empty(),
        "force_drain_all_buffers left the buffered message parked",
    );
}

/// A migrated entity never receives the `Spawn` message whose arm normally
/// performs the auth drain, so without this explicit release an early-arrived
/// authority message parks in the auth sub-channel forever.
#[test]
fn the_migration_release_delivers_messages_that_raced_the_migration() {
    let entity = RemoteEntity::new(1);
    let mut channel = RemoteEntityChannel::new_delegated(HostType::Client);

    // Arrives before the migration upgrade flips the channel to Spawned.
    channel.receive_message(1, EntityMessage::ReleaseAuthority(1, ()));
    let mut before = Vec::new();
    channel.drain_incoming_messages_into(entity, &mut before);
    assert!(
        before.is_empty(),
        "fixture: the message should still be buffered before the release",
    );

    channel.set_spawned(0);
    channel.drain_migration_buffered_messages();

    let mut after = Vec::new();
    channel.drain_incoming_messages_into(entity, &mut after);
    assert!(
        after
            .iter()
            .any(|msg| matches!(msg, EntityMessage::ReleaseAuthority(_, _))),
        "the message that raced the migration was never released: {after:?}",
    );
}

// ---------------------------------------------------------------------------
// The coalesced-spawn state gate.
// ---------------------------------------------------------------------------

/// `SpawnWithComponents` is only legal on a channel that has not spawned yet.
/// The gate deciding that could be inverted unnoticed, which would make the
/// first coalesced spawn stall forever and a repeat spawn re-fire.
#[test]
fn a_coalesced_spawn_spawns_an_unspawned_channel() {
    let entity = RemoteEntity::new(1);
    let mut channel = RemoteEntityChannel::new(HostType::Client);

    channel.receive_message(
        1,
        EntityMessage::SpawnWithComponents((), vec![component_kind::<Alpha>()]),
    );

    let mut events = Vec::new();
    channel.drain_incoming_messages_into(entity, &mut events);

    assert!(
        events
            .iter()
            .any(|msg| matches!(msg, EntityMessage::Spawn(_))),
        "the coalesced spawn never spawned the channel: {events:?}",
    );
    assert!(
        channel.has_component_kind(&component_kind::<Alpha>()),
        "the coalesced components were not registered",
    );
}

/// The other side of the gate: a second coalesced spawn on an already-spawned
/// channel must not spawn it again.
#[test]
fn a_repeated_coalesced_spawn_is_not_applied_twice() {
    let entity = RemoteEntity::new(1);
    let mut channel = RemoteEntityChannel::new(HostType::Client);

    channel.receive_message(
        1,
        EntityMessage::SpawnWithComponents((), vec![component_kind::<Alpha>()]),
    );
    let mut first = Vec::new();
    channel.drain_incoming_messages_into(entity, &mut first);
    assert!(!first.is_empty(), "fixture: the first spawn did not apply");

    channel.receive_message(
        2,
        EntityMessage::SpawnWithComponents((), vec![component_kind::<Alpha>()]),
    );
    let mut second = Vec::new();
    channel.drain_incoming_messages_into(entity, &mut second);

    assert!(
        !second
            .iter()
            .any(|msg| matches!(msg, EntityMessage::Spawn(_))),
        "an already-spawned channel spawned a second time: {second:?}",
    );
}
