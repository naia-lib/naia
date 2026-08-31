//! Unit coverage for `LocalWorldManager`.
//!
//! A sweep found 135 of 172 mutants surviving here. `LocalWorldManager` is a
//! facade: nearly every method forwards to `host`, `remote`, `updater`, or the
//! shared entity map, so a mutant that replaces the body with `()` or a
//! constant is invisible to any test that only checks the layer below. The
//! integration suites drive the whole connection end to end, where a neutered
//! forwarder is masked by whatever else set the same state.

use std::net::SocketAddr;

use crate::{
    world::{
        component::property::Property, local::local_world_manager::LocalWorldManager,
        test_support::TestGwm,
    },
    BigMapKey, ComponentKind, ComponentKinds, GlobalEntity, GlobalEntityIndex, HostEntity,
    HostType, Instant, LocalEntityAndGlobalEntityConverter, OwnedLocalEntity, PacketNotifiable,
    Replicate,
};

#[derive(Replicate)]
struct Ghost {
    value: Property<u8>,
}

#[derive(Replicate)]
struct Wraith {
    value: Property<u8>,
}

fn ghost() -> ComponentKind {
    ComponentKind::of::<Ghost>()
}

fn wraith() -> ComponentKind {
    ComponentKind::of::<Wraith>()
}

fn global(id: u64) -> GlobalEntity {
    GlobalEntity::from_u64(id)
}

struct Fixture {
    kinds: ComponentKinds,
    gwm: TestGwm,
    manager: LocalWorldManager,
}

impl Fixture {
    fn new(host_type: HostType) -> Self {
        let mut kinds = ComponentKinds::new();
        kinds.add_component::<Ghost>();
        kinds.add_component::<Wraith>();
        let gwm = TestGwm::new(&kinds);
        let address: Option<SocketAddr> = Some("127.0.0.1:4000".parse().unwrap());
        let manager = LocalWorldManager::new(&address, host_type, 1, &gwm);
        Self {
            kinds,
            gwm,
            manager,
        }
    }

    fn server() -> Self {
        Self::new(HostType::Server)
    }

    /// Brings `entity` into scope as a host-owned entity carrying `kinds`,
    /// then drains the spawn command it queues.
    fn spawn_host(&mut self, entity: &GlobalEntity, kinds: Vec<ComponentKind>) -> HostEntity {
        self.manager
            .host_init_entity(entity, kinds, &self.kinds, false);
        let _ = self.drain_commands();
        self.manager
            .entity_converter()
            .global_entity_to_host_entity(entity)
            .expect("fixture: the initialised entity should have a host id")
    }

    fn drain_commands(&mut self) -> Vec<crate::EntityCommand> {
        let now = Instant::now();
        self.manager
            .take_outgoing_commands(&now, &200.0)
            .into_iter()
            .map(|(_, command)| command)
            .collect()
    }
}

// -- entity registration ---------------------------------------------------

/// `host_init_entity` has to allocate a host id, register it in the shared
/// entity map, and register the entity with the host engine. A test that only
/// checked the queued command would miss the mapping.
#[test]
fn initialising_a_host_entity_maps_it_and_registers_a_channel() {
    let mut fx = Fixture::server();
    let entity = global(1);

    let host_entity = fx.spawn_host(&entity, vec![ghost()]);

    assert!(fx.manager.has_global_entity(&entity));
    assert!(fx.manager.has_host_entity(&host_entity));
    assert!(fx.manager.get_host_entity_channel(&host_entity).is_some());
}

/// Both `has_*` predicates must consult the registry rather than answer a
/// constant, in both directions.
#[test]
fn the_entity_predicates_answer_for_tracked_entities_only() {
    let mut fx = Fixture::server();
    let host_entity = fx.spawn_host(&global(1), vec![]);
    let untracked = HostEntity::new(200);

    assert!(fx.manager.has_host_entity(&host_entity));
    assert!(!fx.manager.has_host_entity(&untracked));
    assert!(fx.manager.has_global_entity(&global(1)));
    assert!(!fx.manager.has_global_entity(&global(99)));
    assert!(fx.manager.has_local_entity(&OwnedLocalEntity::Host {
        id: host_entity.value(),
        is_static: false,
    }));
    assert!(!fx.manager.has_local_entity(&OwnedLocalEntity::Host {
        id: untracked.value(),
        is_static: false,
    }));
    assert!(!fx.manager.has_local_entity(&OwnedLocalEntity::Remote {
        id: host_entity.value(),
        is_static: false,
    }));
}

/// Both channel lookups have to miss for an entity the manager never saw.
#[test]
fn the_host_channel_lookups_find_only_tracked_entities() {
    let mut fx = Fixture::server();
    let host_entity = fx.spawn_host(&global(1), vec![]);
    let untracked = HostEntity::new(200);

    assert!(fx.manager.get_host_entity_channel(&host_entity).is_some());
    assert!(fx.manager.get_host_entity_channel(&untracked).is_none());
    assert!(fx
        .manager
        .get_host_entity_channel_mut(&host_entity)
        .is_some());
    assert!(fx.manager.get_host_entity_channel_mut(&untracked).is_none());
}

/// A reservation hands out a host id up front; removing it returns that id
/// once and then nothing.
#[test]
fn a_reserved_host_entity_is_returned_once_and_then_gone() {
    let mut fx = Fixture::server();
    let entity = global(1);

    let reserved = fx.manager.host_reserve_entity(&entity);

    assert_eq!(
        fx.manager.host_remove_reserved_entity(&entity),
        Some(reserved)
    );
    assert_eq!(fx.manager.host_remove_reserved_entity(&entity), None);
}

/// A reservation alone does not survive `host_init_entity`. The stale-mapping
/// check treats "mapped, but no live channel" as the residue of a sent-but-
/// unacked despawn and evicts it, and a merely-reserved entity is
/// indistinguishable from that -- so initialisation allocates a fresh id.
///
/// Guarding the eviction on the wrong side of the `!` would silently hand the
/// reserved id back here instead.
#[test]
fn initialising_a_reserved_entity_allocates_a_fresh_id() {
    let mut fx = Fixture::server();
    let entity = global(1);
    let reserved = fx.manager.host_reserve_entity(&entity);

    let host_entity = fx.spawn_host(&entity, vec![ghost()]);

    assert_ne!(
        host_entity, reserved,
        "the stale-mapping check did not evict the reservation",
    );
    assert!(fx.manager.has_host_entity(&host_entity));
    assert!(
        !fx.manager.has_host_entity(&reserved),
        "the reserved id was left registered as a live channel",
    );
}

// -- pause / resume --------------------------------------------------------

/// The pause set is the only state behind all three methods, so each has to
/// actually touch it.
#[test]
fn pausing_and_resuming_moves_an_entity_in_and_out_of_the_pause_set() {
    let mut fx = Fixture::server();
    let entity = global(1);
    assert!(!fx.manager.is_entity_paused(&entity));

    fx.manager.pause_entity(&entity);
    assert!(fx.manager.is_entity_paused(&entity));
    assert!(
        !fx.manager.is_entity_paused(&global(2)),
        "pausing one entity paused another",
    );

    fx.manager.resume_entity(&entity);
    assert!(!fx.manager.is_entity_paused(&entity));
}

/// A despawn clears the pause state too -- a paused entity that goes away must
/// not leave an entry behind for a recycled id to inherit.
#[test]
fn despawning_a_paused_entity_clears_the_pause_state() {
    let mut fx = Fixture::server();
    let entity = global(1);
    fx.spawn_host(&entity, vec![]);
    fx.manager.pause_entity(&entity);

    fx.manager.despawn_entity(&entity);

    assert!(!fx.manager.is_entity_paused(&entity));
}

// -- command routing -------------------------------------------------------

/// Each of the three routers must queue its own command shape for a
/// host-owned entity.
#[test]
fn the_routers_queue_the_matching_command_for_a_host_entity() {
    let mut fx = Fixture::server();
    let entity = global(1);
    fx.spawn_host(&entity, vec![ghost()]);

    fx.manager.insert_component(&entity, &wraith());
    fx.manager.remove_component(&entity, &wraith());
    fx.manager.despawn_entity(&entity);

    let commands = fx.drain_commands();
    assert!(
        commands.contains(&crate::EntityCommand::InsertComponent(entity, wraith())),
        "{commands:?}",
    );
    assert!(
        commands.contains(&crate::EntityCommand::RemoveComponent(entity, wraith())),
        "{commands:?}",
    );
    assert!(
        commands.contains(&crate::EntityCommand::Despawn(entity)),
        "{commands:?}",
    );
}

/// The spawn shape follows the component list: a bare entity gets `Spawn`, a
/// loaded one gets `SpawnWithComponents`.
#[test]
fn the_spawn_shape_follows_the_component_list() {
    let mut fx = Fixture::server();
    fx.manager
        .host_init_entity(&global(1), vec![], &fx.kinds.clone(), false);
    fx.manager
        .host_init_entity(&global(2), vec![ghost()], &fx.kinds.clone(), false);

    let commands = fx.drain_commands();
    assert!(
        commands.contains(&crate::EntityCommand::Spawn(global(1))),
        "{commands:?}",
    );
    assert!(
        commands.iter().any(|c| matches!(
            c,
            crate::EntityCommand::SpawnWithComponents(e, _) if *e == global(2)
        )),
        "{commands:?}",
    );
}

/// Nothing comes out of a manager with nothing queued, and what does come out
/// is the command that was queued -- the sender keeps unacked commands for
/// retransmit, so this is a "did it reach the sender at all" check, not a
/// hand-off.
#[test]
fn only_queued_commands_reach_the_sender() {
    let mut fx = Fixture::server();
    assert!(
        fx.drain_commands().is_empty(),
        "an idle manager produced commands",
    );

    fx.manager
        .host_init_entity(&global(1), vec![], &fx.kinds.clone(), false);

    assert!(fx
        .drain_commands()
        .contains(&crate::EntityCommand::Spawn(global(1))));
}

/// Despawning an entity the map never saw is a programmer error, not a
/// droppable packet.
#[test]
#[should_panic(expected = "does not exist in local entity map")]
fn despawning_an_unmapped_entity_panics() {
    let mut fx = Fixture::server();

    fx.manager.despawn_entity(&global(1));
}

#[test]
#[should_panic(expected = "does not exist in local entity map")]
fn inserting_a_component_on_an_unmapped_entity_panics() {
    let mut fx = Fixture::server();

    fx.manager.insert_component(&global(1), &ghost());
}

#[test]
#[should_panic(expected = "does not exist in local entity map")]
fn removing_a_component_from_an_unmapped_entity_panics() {
    let mut fx = Fixture::server();

    fx.manager.remove_component(&global(1), &ghost());
}

/// `despawn_entity_and_notify_server` returns quietly for an unmapped entity
/// rather than panicking -- it runs on a path where the entity may already be
/// gone.
#[test]
fn notifying_a_despawn_for_an_unmapped_entity_is_a_no_op() {
    let mut fx = Fixture::server();

    fx.manager.despawn_entity_and_notify_server(&global(1));

    assert!(fx.drain_commands().is_empty());
}

/// For a host-owned entity the notify variant queues the same single despawn
/// as the plain one: the extra outgoing despawn is for remote-owned entities
/// the client holds authority over.
#[test]
fn notifying_a_despawn_of_a_host_entity_queues_one_despawn() {
    let mut fx = Fixture::server();
    let entity = global(1);
    fx.spawn_host(&entity, vec![]);

    fx.manager.despawn_entity_and_notify_server(&entity);

    let despawns = fx
        .drain_commands()
        .into_iter()
        .filter(|c| matches!(c, crate::EntityCommand::Despawn(e) if *e == entity))
        .count();
    assert_eq!(despawns, 1, "the notify path double-queued the despawn");
}

// -- update queries --------------------------------------------------------

/// Before the peer has acknowledged the spawn, nothing about the entity is
/// updatable; after `insert_component` registers it and the diff mask is
/// armed, the dirty query flips.
#[test]
fn an_unknown_component_is_neither_updatable_nor_dirty() {
    let fx = Fixture::server();
    let entity = global(1);

    assert!(!fx
        .manager
        .is_component_updatable_for_entity(&entity, &ghost()));
    assert!(!fx
        .manager
        .is_component_dirty_and_delivered_for_entity(&entity, &ghost()));
    assert!(
        fx.manager.diff_mask_is_clear_for_entity(&entity, &ghost()),
        "an unregistered component reported pending bits",
    );
}

/// The dense query is the hot-path twin of the keyed one; with nothing
/// registered it must agree with it rather than answer a constant.
#[test]
fn the_dense_queries_agree_with_the_keyed_ones_when_nothing_is_registered() {
    let fx = Fixture::server();

    assert!(!fx
        .manager
        .is_component_dirty_and_delivered_dense(GlobalEntityIndex(0), 0));
    assert!(fx.manager.diff_mask_is_clear_dense(GlobalEntityIndex(0), 0));
    assert!(fx
        .manager
        .get_diff_mask_dense(GlobalEntityIndex(0), 0)
        .is_none());
}

// -- packet bookkeeping ----------------------------------------------------

/// `collect_messages` drops command-packet records older than the TTL. The
/// record is only reachable through `record_command_written`, which panics if
/// `insert_sent_command_packet` did not run first -- so a dropped record is
/// observable as that panic.
#[test]
#[should_panic(expected = "called `Option::unwrap()` on a `None` value")]
fn collecting_messages_retires_command_packets_past_the_ttl() {
    let mut fx = Fixture::server();
    let sent_at = Instant::now();
    let mut much_later = sent_at.clone();
    much_later.add_millis(120_000);
    fx.manager.insert_sent_command_packet(&0, sent_at);
    fx.manager.collect_messages(&much_later, &200.0);

    fx.manager
        .record_command_written(&0, &0, crate::EntityMessage::Noop);
}

/// A record inside the TTL survives the sweep, so the write lands.
#[test]
fn a_fresh_command_packet_survives_the_ttl_sweep() {
    let mut fx = Fixture::server();
    let sent_at = Instant::now();
    let mut soon = sent_at.clone();
    soon.add_millis(1_000);
    fx.manager.insert_sent_command_packet(&0, sent_at);
    fx.manager.collect_messages(&soon, &200.0);

    fx.manager
        .record_command_written(&0, &0, crate::EntityMessage::Noop);
}

/// Re-inserting the same packet index must not create a second record, or the
/// TTL sweep would leave a duplicate behind.
#[test]
fn inserting_the_same_command_packet_twice_keeps_one_record() {
    let mut fx = Fixture::server();
    let sent_at = Instant::now();

    let mut much_later = sent_at.clone();
    much_later.add_millis(120_000);
    let mut mid = sent_at.clone();
    mid.add_millis(90_000);
    fx.manager.insert_sent_command_packet(&0, sent_at);
    fx.manager.insert_sent_command_packet(&0, much_later);

    // If the second insert had replaced the record with the later timestamp,
    // this sweep would leave it in place.
    fx.manager.collect_messages(&mid, &200.0);
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        fx.manager
            .record_command_written(&0, &0, crate::EntityMessage::Noop);
    }))
    .is_err();
    assert!(panicked, "the duplicate insert kept the record alive");
}

// -- diagnostics -----------------------------------------------------------

/// A fresh manager tracks no remote entities, and a host-owned one does not
/// count as remote.
#[test]
fn the_remote_entity_list_holds_only_remote_entities() {
    let mut fx = Fixture::server();
    assert!(fx.manager.remote_entities().is_empty());

    fx.spawn_host(&global(1), vec![]);

    assert!(
        fx.manager.remote_entities().is_empty(),
        "a host-owned entity was reported as remote",
    );
}

/// Nothing is pending outbound until an entity is initialised, and the entity
/// that is initialised is the one reported.
#[test]
fn the_pending_outbound_set_follows_initialisation() {
    let mut fx = Fixture::server();
    assert_eq!(fx.manager.pending_outbound_entities().count(), 0);

    fx.manager
        .host_init_entity(&global(1), vec![ghost()], &fx.kinds.clone(), false);

    let pending: Vec<GlobalEntity> = fx.manager.pending_outbound_entities().collect();
    assert_eq!(pending, vec![global(1)]);
}

/// Translates the commands a `take_outgoing_commands` drain produced into the
/// wire messages `world_writer` would have recorded for them, and files them
/// against `packet_index` -- the minimum needed to make a delivery ack
/// meaningful without pulling in the whole packet writer.
impl Fixture {
    fn record_written(
        &mut self,
        packet_index: &crate::PacketIndex,
        commands: std::collections::VecDeque<(
            crate::world::host::host_world_manager::CommandId,
            crate::EntityCommand,
        )>,
    ) {
        for (command_id, command) in commands {
            let message = match command {
                crate::EntityCommand::Spawn(global_entity) => {
                    crate::EntityMessage::Spawn(self.owned(&global_entity))
                }
                crate::EntityCommand::SpawnWithComponents(global_entity, kinds) => {
                    crate::EntityMessage::SpawnWithComponents(self.owned(&global_entity), kinds)
                }
                crate::EntityCommand::InsertComponent(global_entity, kind) => {
                    crate::EntityMessage::InsertComponent(self.owned(&global_entity), kind)
                }
                other => panic!("fixture: no translation for {other:?}"),
            };
            self.manager
                .record_command_written(packet_index, &command_id, message);
        }
    }

    fn owned(&self, global_entity: &GlobalEntity) -> OwnedLocalEntity {
        self.manager
            .entity_converter()
            .global_entity_to_owned_entity(global_entity)
            .expect("fixture: the entity should be mapped")
    }
}

// -- host-side extraction ------------------------------------------------

#[test]
fn the_host_component_kinds_come_from_the_channel() {
    let mut fx = Fixture::server();
    let entity = global(1);
    fx.spawn_host(&entity, vec![ghost(), wraith()]);

    let kinds = fx.manager.extract_host_component_kinds(&entity);
    assert_eq!(
        kinds,
        std::collections::HashSet::from([ghost(), wraith()]),
        "the channel should report exactly the kinds it was initialised with"
    );
}

/// The per-channel queue is drained into the engine-level buffer by
/// `send_command` itself, so on the ordinary path extraction finds nothing --
/// it exists for the migration path in `client.rs`, where the channel is
/// re-read after the engine buffer has already been taken.
#[test]
fn extracting_the_host_commands_finds_an_already_drained_channel_empty() {
    let mut fx = Fixture::server();
    let entity = global(1);
    fx.spawn_host(&entity, vec![ghost()]);

    fx.manager.insert_component(&entity, &wraith());
    assert!(
        fx.manager.extract_host_entity_commands(&entity).is_empty(),
        "send_command already forwarded the insert to the engine buffer"
    );
    assert!(
        fx.drain_commands().iter().any(|command| matches!(
            command,
            crate::EntityCommand::InsertComponent(e, k) if *e == entity && *k == wraith()
        )),
        "and the insert should be waiting there instead"
    );
}

#[test]
#[should_panic(expected = "EntityDoesNotExistError")]
fn extracting_the_host_commands_of_an_unmapped_entity_panics() {
    let mut fx = Fixture::server();
    let _ = fx.manager.extract_host_entity_commands(&global(9));
}

#[test]
fn removing_a_host_entity_drops_both_the_channel_and_the_mapping() {
    let mut fx = Fixture::server();
    let entity = global(1);
    let host_entity = fx.spawn_host(&entity, vec![ghost()]);

    fx.manager.remove_host_entity(&entity);

    assert!(
        !fx.manager.has_host_entity(&host_entity),
        "the host engine channel should be gone"
    );
    assert!(
        fx.manager
            .entity_converter()
            .global_entity_to_host_entity(&entity)
            .is_err(),
        "and so should the entity map entry"
    );
}

// -- delivery ------------------------------------------------------------

#[test]
fn delivering_a_packet_retires_the_commands_it_carried() {
    let mut fx = Fixture::server();
    let entity = global(1);
    fx.manager
        .host_init_entity(&entity, vec![ghost()], &fx.kinds, false);

    let now = Instant::now();
    let outgoing = fx.manager.take_outgoing_commands(&now, &200.0);
    assert!(
        !outgoing.is_empty(),
        "fixture: the spawn should have produced a command to deliver"
    );

    let packet_index: crate::PacketIndex = 0;
    fx.manager.insert_sent_command_packet(&packet_index, now);
    fx.record_written(&packet_index, outgoing);

    assert!(
        fx.manager.pending_outbound_entities().any(|e| e == entity),
        "before the ack the spawn is still in flight"
    );

    PacketNotifiable::notify_packet_delivered(&mut fx.manager, packet_index);
    fx.manager.process_delivered_commands();

    assert!(
        !fx.manager.pending_outbound_entities().any(|e| e == entity),
        "the ack should have retired the in-flight spawn"
    );
}

#[test]
fn delivering_an_unknown_packet_is_a_no_op() {
    let mut fx = Fixture::server();
    let entity = global(1);
    fx.manager
        .host_init_entity(&entity, vec![ghost()], &fx.kinds, false);
    let now = Instant::now();
    let _ = fx.manager.take_outgoing_commands(&now, &200.0);

    PacketNotifiable::notify_packet_delivered(&mut fx.manager, 9);
    fx.manager.process_delivered_commands();

    assert!(
        fx.manager.pending_outbound_entities().any(|e| e == entity),
        "an ack for a packet we never sent must not retire anything"
    );
}

#[test]
fn patching_the_in_flight_refs_redirects_the_delivery() {
    let mut fx = Fixture::server();
    let entity = global(1);
    let host_entity = fx.spawn_host(&entity, vec![ghost()]);

    fx.manager.insert_component(&entity, &wraith());
    let now = Instant::now();
    let outgoing = fx.manager.take_outgoing_commands(&now, &200.0);
    let packet_index: crate::PacketIndex = 0;
    fx.manager.insert_sent_command_packet(&packet_index, now);
    fx.record_written(&packet_index, outgoing);

    // Point every in-flight reference at an entity the host engine has never
    // heard of. Delivery must follow the patch, so the real channel is left
    // untouched -- if the patch were a no-op the insert would retire here.
    let stranger = HostEntity::new(250).copy_to_owned();
    fx.manager
        .update_sent_command_entity_refs(&entity, host_entity.copy_to_owned(), stranger);
    PacketNotifiable::notify_packet_delivered(&mut fx.manager, packet_index);
    fx.manager.process_delivered_commands();

    assert!(
        fx.manager.pending_outbound_entities().any(|e| e == entity),
        "the delivery was redirected away, so the entity stays in flight"
    );
}

// -- publish and delegation ----------------------------------------------

/// Command types a drain produced, so the assertions below read as sequences
/// rather than as nested `matches!` chains.
fn command_types(commands: &[crate::EntityCommand]) -> Vec<crate::EntityMessageType> {
    commands.iter().map(|command| command.get_type()).collect()
}

/// The `ReliableSender` retains unacked commands for retransmit, so every drain
/// replays the fixture's own spawn. Only what follows it is this test's doing.
fn types_after_the_spawn(commands: &[crate::EntityCommand]) -> Vec<crate::EntityMessageType> {
    command_types(commands)
        .into_iter()
        .skip_while(|t| {
            matches!(
                t,
                crate::EntityMessageType::Spawn | crate::EntityMessageType::SpawnWithComponents
            )
        })
        .collect()
}

#[test]
fn a_client_publishes_its_own_host_entity_through_the_host_engine() {
    let mut fx = Fixture::new(HostType::Client);
    let entity = global(1);
    fx.spawn_host(&entity, vec![ghost()]);

    fx.manager.send_publish(HostType::Client, &entity);
    assert_eq!(
        types_after_the_spawn(&fx.drain_commands()),
        vec![crate::EntityMessageType::Publish],
        "the publish should reach the sender as a Publish command"
    );

    fx.manager.send_unpublish(HostType::Client, &entity);
    assert_eq!(
        types_after_the_spawn(&fx.drain_commands()),
        vec![
            crate::EntityMessageType::Publish,
            crate::EntityMessageType::Unpublish
        ],
        "and the unpublish likewise"
    );
}

#[test]
#[should_panic(expected = "published by default")]
fn a_server_cannot_publish_its_own_entity() {
    let mut fx = Fixture::server();
    let entity = global(1);
    fx.spawn_host(&entity, vec![ghost()]);
    fx.manager.send_publish(HostType::Server, &entity);
}

#[test]
#[should_panic(expected = "cannot be unpublished")]
fn a_server_cannot_unpublish_its_own_entity() {
    let mut fx = Fixture::server();
    let entity = global(1);
    fx.spawn_host(&entity, vec![ghost()]);
    fx.manager.send_unpublish(HostType::Server, &entity);
}

/// A server-owned host channel starts in `Published`, so the publish is
/// skipped and `EnableDelegation` goes out alone. Every mutation of that
/// `is_published` test -- flipping either `==`, weakening `||` to `&&`, or
/// dropping the `!` -- adds a redundant `Publish` ahead of it.
#[test]
fn enabling_delegation_on_an_already_published_entity_skips_the_publish() {
    let mut fx = Fixture::server();
    let entity = global(1);
    fx.spawn_host(&entity, vec![ghost()]);

    fx.manager
        .send_enable_delegation(HostType::Server, false, &entity);

    assert_eq!(
        types_after_the_spawn(&fx.drain_commands()),
        vec![crate::EntityMessageType::EnableDelegation],
        "an already-published entity needs no second Publish"
    );
}

#[test]
fn disabling_delegation_queues_one_command() {
    let mut fx = Fixture::server();
    let entity = global(1);
    fx.spawn_host(&entity, vec![ghost()]);

    fx.manager
        .send_enable_delegation(HostType::Server, false, &entity);
    fx.manager.send_disable_delegation(&entity);
    assert_eq!(
        types_after_the_spawn(&fx.drain_commands()),
        vec![
            crate::EntityMessageType::EnableDelegation,
            crate::EntityMessageType::DisableDelegation
        ],
    );
}

#[test]
#[should_panic(expected = "must be the owning client")]
fn a_client_must_own_the_entity_to_enable_delegation() {
    let mut fx = Fixture::new(HostType::Client);
    let entity = global(1);
    fx.spawn_host(&entity, vec![ghost()]);
    fx.manager
        .send_enable_delegation(HostType::Client, false, &entity);
}

#[test]
fn releasing_authority_on_a_host_entity_goes_out_through_the_host_engine() {
    let mut fx = Fixture::server();
    let entity = global(1);
    fx.spawn_host(&entity, vec![ghost()]);
    fx.manager
        .send_enable_delegation(HostType::Server, false, &entity);
    let _ = fx.drain_commands();

    fx.manager.remote_send_release_auth(&entity);
    assert_eq!(
        types_after_the_spawn(&fx.drain_commands()),
        vec![
            crate::EntityMessageType::EnableDelegation,
            crate::EntityMessageType::ReleaseAuthority
        ],
    );
}

// -- authority registration and the live diff mask ------------------------

/// Registers `entity`/`kind` everywhere the update ledger needs it, then grants
/// authority -- which marks the component fully dirty. Returns nothing; the
/// caller asserts on the manager's mask queries.
impl Fixture {
    fn arm_and_grant(&mut self, entity: &GlobalEntity, kind: &ComponentKind) {
        self.gwm.arm_diff_handler(&self.kinds, entity, kind);
        self.gwm.declare_kinds(entity, vec![*kind]);
        self.manager.insert_component(entity, kind);
        let _ = self.drain_commands();
        self.manager.register_authed_entity(&self.gwm, entity);
    }
}

#[test]
fn granting_authority_dirties_every_declared_component() {
    let mut fx = Fixture::server();
    let entity = global(1);
    fx.spawn_host(&entity, vec![]);

    assert!(
        fx.manager.diff_mask_is_clear_for_entity(&entity, &ghost()),
        "before the grant there is nothing pending"
    );

    fx.arm_and_grant(&entity, &ghost());

    assert!(
        !fx.manager.diff_mask_is_clear_for_entity(&entity, &ghost()),
        "the grant republishes full state, so every bit should be set"
    );
}

#[test]
fn revoking_authority_stops_tracking_the_components() {
    let mut fx = Fixture::server();
    let entity = global(1);
    fx.spawn_host(&entity, vec![]);
    fx.arm_and_grant(&entity, &ghost());

    fx.manager.deregister_authed_entity(&fx.gwm, &entity);

    assert!(
        fx.manager.diff_mask_is_clear_for_entity(&entity, &ghost()),
        "a deregistered component has no live mask left to report"
    );
}

#[test]
fn an_entity_with_no_declared_components_is_a_no_op_for_both_directions() {
    let mut fx = Fixture::server();
    let entity = global(1);
    fx.spawn_host(&entity, vec![ghost()]);
    // `declare_kinds` is never called, so the manager reports `None` kinds.

    fx.manager.register_authed_entity(&fx.gwm, &entity);
    assert!(
        fx.manager.diff_mask_is_clear_for_entity(&entity, &ghost()),
        "with nothing declared the grant has nothing to dirty"
    );
    fx.manager.deregister_authed_entity(&fx.gwm, &entity);
}

#[test]
fn recording_an_update_clears_the_live_mask() {
    let mut fx = Fixture::server();
    let entity = global(1);
    fx.spawn_host(&entity, vec![]);
    fx.arm_and_grant(&entity, &ghost());

    let now = Instant::now();
    fx.manager
        .record_update(&now, &0, &entity, &ghost(), crate::DiffMask::new(1));

    assert!(
        fx.manager.diff_mask_is_clear_for_entity(&entity, &ghost()),
        "the client path records the ledger entry AND clears the live mask"
    );
}

#[test]
fn recording_a_sent_update_leaves_the_live_mask_alone() {
    let mut fx = Fixture::server();
    let entity = global(1);
    fx.spawn_host(&entity, vec![]);
    fx.arm_and_grant(&entity, &ghost());

    let now = Instant::now();
    fx.manager
        .record_sent_update(&now, &0, &entity, &ghost(), crate::DiffMask::new(1));

    assert!(
        !fx.manager.diff_mask_is_clear_for_entity(&entity, &ghost()),
        "the server path clears up-front in prepare_send_job, never here"
    );
}

#[test]
fn the_dense_clear_reaches_the_same_mask_as_the_keyed_query() {
    let mut fx = Fixture::server();
    let entity = global(1);
    fx.spawn_host(&entity, vec![]);
    fx.arm_and_grant(&entity, &ghost());

    let (entity_idx, kind_bit) = {
        let gdh = fx.gwm.diff_handler.read().unwrap();
        (
            gdh.entity_to_global_idx(&entity)
                .expect("the armed entity should have a dense index"),
            gdh.kind_bit(&ghost())
                .expect("the armed component should have a kind bit"),
        )
    };

    assert!(
        !fx.manager.diff_mask_is_clear_dense(entity_idx, kind_bit),
        "the dense query should see the same pending bits as the keyed one"
    );

    fx.manager.clear_diff_mask_dense(entity_idx, kind_bit);

    assert!(
        fx.manager.diff_mask_is_clear_for_entity(&entity, &ghost()),
        "and the dense clear should be visible through the keyed query"
    );
}
