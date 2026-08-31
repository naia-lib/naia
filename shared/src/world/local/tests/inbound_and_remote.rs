//! Inbound-buffer, remote-engine and redirect coverage for [`LocalWorldManager`].
//!
//! These exercise the paths that need a real world to observe: an update is
//! only meaningfully "taken" if it lands in a component someone can read back.

use std::{collections::HashSet, net::SocketAddr};

use crate::{
    world::{
        component::property::Property,
        local::local_world_manager::LocalWorldManager,
        test_support::TestGwm,
        test_world::{full_update, remote_component, IdentityConverter, TestSpawner, TestWorld},
    },
    BigMapKey, ComponentKind, ComponentKinds, GlobalEntity, HostType,
    LocalEntityAndGlobalEntityConverter, OwnedLocalEntity, RemoteEntity, Replicate, WorldMutType,
};

#[derive(Replicate)]
struct Ghost {
    value: Property<u8>,
}

#[derive(Replicate)]
struct Wraith {
    value: Property<u8>,
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
    fn client() -> Self {
        let mut kinds = ComponentKinds::new();
        kinds.add_component::<Ghost>();
        kinds.add_component::<Wraith>();
        let gwm = TestGwm::new(&kinds);
        let address: Option<SocketAddr> = Some("127.0.0.1:4000".parse().unwrap());
        let manager = LocalWorldManager::new(&address, HostType::Client, 1, &gwm);
        Self {
            kinds,
            gwm,
            manager,
        }
    }

    /// Registers `id` as a remote entity holding `Ghost`, and spawns the same
    /// id in `world` so the two sides agree.
    fn adopt_remote(&mut self, world: &mut TestWorld, id: u64) -> OwnedLocalEntity {
        let remote_entity = RemoteEntity::new(id as u32);
        let mut component_kinds = HashSet::new();
        component_kinds.insert(ComponentKind::of::<Ghost>());
        self.manager
            .insert_remote_entity(&global(id), remote_entity, component_kinds);
        world.spawn_at(id);
        world.insert_boxed_component(&id, remote_component(&self.kinds, &Ghost::new_complete(0)));
        remote_entity.copy_to_owned()
    }

    /// The command types currently sitting in the sender. Note the sender
    /// retains unacked commands, so successive drains are cumulative.
    fn drain_command_types(&mut self) -> Vec<crate::EntityMessageType> {
        let now = crate::Instant::now();
        self.manager
            .take_outgoing_commands(&now, &200.0)
            .into_iter()
            .map(|(_, command)| command.get_type())
            .collect()
    }

    fn take_events(&mut self, world: &mut TestWorld) -> Vec<crate::EntityEvent> {
        let mut spawner = TestSpawner::new();
        let now = crate::Instant::now();
        let kinds = std::mem::replace(&mut self.kinds, ComponentKinds::new());
        let events =
            self.manager
                .take_incoming_events(&mut spawner, &self.gwm, &kinds, world, &now);
        self.kinds = kinds;
        events
    }
}

// -- the incoming update buffer ------------------------------------------

#[test]
fn a_buffered_update_is_drained_and_applied_to_the_world() {
    let mut fx = Fixture::client();
    let mut world = TestWorld::new();
    let owned = fx.adopt_remote(&mut world, 7);

    let update = full_update(&fx.kinds, &Ghost::new_complete(42));
    fx.manager.insert_received_update(3, &owned, update);

    let taken = fx
        .manager
        .take_received_updates_of_kind::<u64, TestWorld, Ghost>(&IdentityConverter, &mut world);

    assert_eq!(
        taken.len(),
        1,
        "the one buffered Ghost update should come back"
    );
    assert_eq!(taken[0].0, 3, "the tick should be carried through verbatim");
    assert_eq!(taken[0].1, 7, "the world entity should be the mapped one");
    assert_eq!(
        *taken[0].2.value, 42,
        "the returned copy should hold the new value"
    );
    assert_eq!(
        *world
            .value_of::<Ghost>(&7)
            .expect("the world still holds Ghost")
            .value,
        42,
        "the update should have been applied to the world, not merely decoded"
    );
}

#[test]
fn updates_of_other_kinds_are_left_in_the_buffer() {
    let mut fx = Fixture::client();
    let mut world = TestWorld::new();
    let owned = fx.adopt_remote(&mut world, 7);
    world.insert_boxed_component(&7, remote_component(&fx.kinds, &Wraith::new_complete(0)));

    fx.manager
        .insert_received_update(1, &owned, full_update(&fx.kinds, &Wraith::new_complete(9)));

    let ghosts = fx
        .manager
        .take_received_updates_of_kind::<u64, TestWorld, Ghost>(&IdentityConverter, &mut world);
    assert!(
        ghosts.is_empty(),
        "a Wraith update must not answer a Ghost drain"
    );
    assert_eq!(
        *world
            .value_of::<Wraith>(&7)
            .expect("Wraith is present")
            .value,
        0,
        "a non-matching update must not be applied while it waits its turn"
    );

    let wraiths = fx
        .manager
        .take_received_updates_of_kind::<u64, TestWorld, Wraith>(&IdentityConverter, &mut world);
    assert_eq!(
        wraiths.len(),
        1,
        "the skipped update should have been kept for its own kind"
    );
    assert_eq!(*wraiths[0].2.value, 9);
}

#[test]
fn a_drained_update_is_not_returned_a_second_time() {
    let mut fx = Fixture::client();
    let mut world = TestWorld::new();
    let owned = fx.adopt_remote(&mut world, 7);

    fx.manager
        .insert_received_update(1, &owned, full_update(&fx.kinds, &Ghost::new_complete(5)));
    let _ = fx
        .manager
        .take_received_updates_of_kind::<u64, TestWorld, Ghost>(&IdentityConverter, &mut world);

    let again = fx
        .manager
        .take_received_updates_of_kind::<u64, TestWorld, Ghost>(&IdentityConverter, &mut world);
    assert!(
        again.is_empty(),
        "a drained update must not be re-applied later"
    );
}

#[test]
fn an_update_for_an_unmapped_entity_is_dropped() {
    let mut fx = Fixture::client();
    let mut world = TestWorld::new();
    let _ = fx.adopt_remote(&mut world, 7);

    let stranger = RemoteEntity::new(99).copy_to_owned();
    fx.manager.insert_received_update(
        1,
        &stranger,
        full_update(&fx.kinds, &Ghost::new_complete(5)),
    );

    let taken = fx
        .manager
        .take_received_updates_of_kind::<u64, TestWorld, Ghost>(&IdentityConverter, &mut world);
    assert!(
        taken.is_empty(),
        "an update for an unknown entity yields nothing"
    );

    let again = fx
        .manager
        .take_received_updates_of_kind::<u64, TestWorld, Ghost>(&IdentityConverter, &mut world);
    assert!(
        again.is_empty(),
        "and it should be discarded, not retained to be retried forever"
    );
}

#[test]
fn updates_are_applied_in_buffer_order() {
    let mut fx = Fixture::client();
    let mut world = TestWorld::new();
    let owned = fx.adopt_remote(&mut world, 7);

    for (tick, value) in [(1u16, 10u8), (2, 20), (3, 30)] {
        fx.manager.insert_received_update(
            tick,
            &owned,
            full_update(&fx.kinds, &Ghost::new_complete(value)),
        );
    }

    let taken = fx
        .manager
        .take_received_updates_of_kind::<u64, TestWorld, Ghost>(&IdentityConverter, &mut world);
    let series: Vec<(u16, u8)> = taken.iter().map(|(t, _, r)| (*t, *r.value)).collect();
    assert_eq!(
        series,
        vec![(1, 10), (2, 20), (3, 30)],
        "the per-tick series is the point: it must arrive in order, not collapsed"
    );
    assert_eq!(
        *world.value_of::<Ghost>(&7).unwrap().value,
        30,
        "the world should be left holding the last applied value"
    );
}

// -- the remote engine ---------------------------------------------------

#[test]
fn inserting_a_remote_entity_maps_it_and_opens_a_channel() {
    let mut fx = Fixture::client();
    let mut world = TestWorld::new();
    let _ = fx.adopt_remote(&mut world, 7);

    let converter = fx.manager.entity_converter();
    assert_eq!(
        converter
            .global_entity_to_remote_entity(&global(7))
            .expect("the global entity should be mapped"),
        RemoteEntity::new(7),
        "the entity map should resolve the global entity to its remote id"
    );
    drop(converter);

    assert!(
        fx.manager
            .get_remote_entity_auth_status(&global(7))
            .is_some(),
        "the remote engine should have opened a delegated channel for it"
    );
}

/// NOTE: `remote_despawn_entity`'s doc comment claims it "cleans up the entity
/// map", but `RemoteWorldManager::despawn_entity` takes the map as
/// `_local_entity_map` and only notifies the waitlist. This pins the behaviour
/// that actually ships; the doc comment is the thing that is wrong.
#[test]
fn despawning_a_remote_entity_notifies_the_waitlist_but_keeps_the_mapping() {
    let mut fx = Fixture::client();
    let mut world = TestWorld::new();
    let _ = fx.adopt_remote(&mut world, 7);

    fx.manager.remote_despawn_entity(&global(7));

    let converter = fx.manager.entity_converter();
    assert!(
        converter.global_entity_to_remote_entity(&global(7)).is_ok(),
        "the entity map entry survives the remote despawn"
    );
}

// -- redirects -----------------------------------------------------------

#[test]
fn an_installed_redirect_retargets_lookups() {
    let mut fx = Fixture::client();
    let old = RemoteEntity::new(3).copy_to_owned();
    let new = RemoteEntity::new(4).copy_to_owned();

    assert_eq!(
        fx.manager.apply_entity_redirect(old),
        old,
        "with no redirect installed the entity comes back untouched"
    );

    fx.manager.install_entity_redirect(old, new);
    assert_eq!(
        fx.manager.apply_entity_redirect(old),
        new,
        "an installed redirect should retarget the old entity"
    );
    assert_eq!(
        fx.manager.apply_entity_redirect(new),
        new,
        "and must not redirect the target onto itself again"
    );
}

// -- the remote-side auth senders ----------------------------------------

#[test]
fn the_remote_auth_senders_each_queue_their_own_command() {
    let mut fx = Fixture::client();
    let mut world = TestWorld::new();
    let _ = fx.adopt_remote(&mut world, 7);
    let entity = global(7);

    fx.manager.remote_send_request_auth(&entity);
    assert_eq!(
        fx.drain_command_types(),
        vec![crate::EntityMessageType::RequestAuthority],
    );

    fx.manager.send_enable_delegation_response(&entity);
    assert_eq!(
        fx.drain_command_types(),
        vec![
            crate::EntityMessageType::RequestAuthority,
            crate::EntityMessageType::EnableDelegationResponse
        ],
        "the sender retains unacked commands, so the first one replays"
    );
}

#[test]
fn receiving_a_set_authority_updates_the_channels_auth_status() {
    let mut fx = Fixture::client();
    let mut world = TestWorld::new();
    let _ = fx.adopt_remote(&mut world, 7);
    let entity = global(7);

    fx.manager
        .remote_receive_set_auth(&entity, crate::EntityAuthStatus::Granted);
    assert_eq!(
        fx.manager.get_remote_entity_auth_status(&entity),
        Some(crate::EntityAuthStatus::Granted),
        "the channel should report the status it was just handed"
    );
}

#[test]
fn the_remote_entity_list_names_the_adopted_entity() {
    let mut fx = Fixture::client();
    let mut world = TestWorld::new();
    let _ = fx.adopt_remote(&mut world, 7);

    assert_eq!(
        fx.manager.remote_entities(),
        vec![global(7)],
        "the entity map should list the one remote entity it holds"
    );
}

// -- the inbound message path --------------------------------------------

#[test]
fn a_buffered_despawn_surfaces_as_one_event_and_noops_are_skipped() {
    let mut fx = Fixture::client();
    let mut world = TestWorld::new();
    let owned = fx.adopt_remote(&mut world, 7);

    fx.manager
        .receiver_buffer_message(0, crate::EntityMessage::Noop);
    fx.manager
        .receiver_buffer_message(1, crate::EntityMessage::Despawn(owned));

    let events = fx.take_events(&mut world);
    assert_eq!(
        events.len(),
        1,
        "the noop should be dropped and the despawn kept"
    );
    assert!(
        matches!(&events[0], crate::EntityEvent::Despawn(e) if *e == global(7)),
        "the surviving event should be the despawn"
    );
}

#[test]
fn nothing_buffered_means_no_events() {
    let mut fx = Fixture::client();
    let mut world = TestWorld::new();
    let _ = fx.adopt_remote(&mut world, 7);

    assert!(
        fx.take_events(&mut world).is_empty(),
        "an idle manager should surface nothing"
    );
}

#[test]
fn a_buffered_noop_alone_surfaces_nothing() {
    let mut fx = Fixture::client();
    let mut world = TestWorld::new();
    let _ = fx.adopt_remote(&mut world, 7);

    fx.manager
        .receiver_buffer_message(0, crate::EntityMessage::Noop);
    assert!(
        fx.take_events(&mut world).is_empty(),
        "a noop carries no entity, so skipping it must come before the entity lookup"
    );
}

// -- the waitlist forwarders ----------------------------------------------

/// `entity_waitlist_queue` parks a message until every remote entity it names
/// is in scope; `remote_spawn_entity` is what later releases it. Both are pure
/// forwarders, so the only way to see them work is to queue, spawn, and then
/// collect the item back out of the store.
#[test]
fn a_queued_message_is_released_once_its_remote_entity_spawns() {
    let mut fx = Fixture::client();
    let mut world = TestWorld::new();
    // Deliberately NOT adopted yet: the message has to wait for it.
    let remote_entity = RemoteEntity::new(3);

    let mut store: crate::world::remote::remote_entity_waitlist::WaitlistStore<u32> =
        crate::world::remote::remote_entity_waitlist::WaitlistStore::new();
    let mut required = HashSet::new();
    required.insert(remote_entity);
    fx.manager.entity_waitlist_queue(&required, &mut store, 42);

    let now = crate::Instant::now();
    assert!(
        fx.manager
            .entity_waitlist_mut()
            .collect_ready_items(&now, &mut store)
            .is_none(),
        "nothing is in scope yet, so nothing should be collectable"
    );

    fx.adopt_remote(&mut world, 3);
    fx.manager.remote_spawn_entity(&global(3));

    let items = fx
        .manager
        .entity_waitlist_mut()
        .collect_ready_items(&now, &mut store)
        .expect("the spawn should have released the queued message");
    assert_eq!(items, vec![42]);
}

/// `remote_spawn_entity` swallows the lookup failure on purpose: a despawn
/// earlier in the same batch can have removed the mapping already. It must not
/// panic, and it must release nothing.
#[test]
fn spawning_a_remote_entity_the_map_no_longer_knows_is_a_no_op() {
    let mut fx = Fixture::client();

    fx.manager.remote_spawn_entity(&global(404));

    let now = crate::Instant::now();
    let mut store: crate::world::remote::remote_entity_waitlist::WaitlistStore<u32> =
        crate::world::remote::remote_entity_waitlist::WaitlistStore::new();
    assert!(
        fx.manager
            .entity_waitlist_mut()
            .collect_ready_items(&now, &mut store)
            .is_none(),
        "an unmapped entity releases nothing"
    );
}

// -- the remote despawn / replay forwarders -------------------------------

/// An authority-holding client owes the server an explicit `Despawn`; the
/// `== Granted` comparison in `despawn_entity_and_notify_server` is the whole
/// decision, so both sides of it need a case.
#[test]
fn an_authority_holding_client_tells_the_server_about_its_despawn() {
    let mut fx = Fixture::client();
    let mut world = TestWorld::new();
    fx.adopt_remote(&mut world, 3);
    fx.manager
        .remote_receive_set_auth(&global(3), crate::EntityAuthStatus::Granted);
    let _ = fx.drain_command_types();

    fx.manager.despawn_entity_and_notify_server(&global(3));

    assert!(
        fx.drain_command_types()
            .contains(&crate::EntityMessageType::Despawn),
        "a granted client has to tell the server it is going away"
    );
}

/// `remote_despawn_entity` notifies the waitlist and resolves the global id to
/// a remote one on the way. It does NOT clear the entity map -- despite the
/// doc comment on `RemoteWorldManager::despawn_entity`, that function binds
/// `_local_entity_map` and ignores it. What is observable is the resolution
/// step: an unmapped global id unwraps to a panic.
#[test]
#[should_panic(expected = "EntityDoesNotExistError")]
fn despawning_a_remote_entity_the_map_never_knew_panics() {
    let mut fx = Fixture::client();

    fx.manager.remote_despawn_entity(&global(404));
}

/// `replay_entity_command` re-submits a command through the remote engine
/// after a migration. A `Despawn` retires the engine's channel, so a second
/// replay of the same command has nothing left to address -- which is how a
/// gutted body shows itself: it would leave the channel standing.
#[test]
#[should_panic(expected = "does not exist in the engine")]
fn replaying_a_despawn_retires_the_remote_channel() {
    let mut fx = Fixture::client();
    let mut world = TestWorld::new();
    fx.adopt_remote(&mut world, 3);

    fx.manager
        .replay_entity_command(&global(3), crate::EntityCommand::Despawn(global(3)));
    fx.manager
        .replay_entity_command(&global(3), crate::EntityCommand::Despawn(global(3)));
}

/// A component that arrives in a scope-entry bundle is buffered in
/// `incoming_components` and applied on the next `take_incoming_events`. The
/// buffer is the only thing `insert_received_component` does, so an emptied
/// body loses the component silently.
#[test]
fn a_received_component_is_buffered_and_then_applied_to_the_world() {
    let mut fx = Fixture::client();
    let mut world = TestWorld::new();
    let local_entity = fx.adopt_remote(&mut world, 3);

    fx.manager.insert_received_component(
        &local_entity,
        &ComponentKind::of::<Wraith>(),
        remote_component(&fx.kinds, &Wraith::new_complete(9)),
    );
    // The buffer is only drained when the matching InsertComponent message is
    // processed, so the message has to arrive too.
    fx.manager.receiver_buffer_message(
        1,
        crate::EntityMessage::InsertComponent(local_entity, ComponentKind::of::<Wraith>()),
    );
    let events = fx.take_events(&mut world);

    assert!(
        events
            .iter()
            .any(|e| matches!(e, crate::EntityEvent::InsertComponent(_, kind)
                if *kind == ComponentKind::of::<Wraith>())),
        "the buffered component should surface as an insert event"
    );
    assert_eq!(
        world.value_of::<Wraith>(&3).map(|w| *w.value),
        Some(9),
        "and it should have landed in the world"
    );
}
