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
        test_world::{full_update, remote_component, IdentityConverter, TestWorld},
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
        Self { kinds, manager }
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
