//! Coverage for the entity-migration and delegation-routing paths of
//! [`LocalWorldManager`].
//!
//! `migrate_entity_remote_to_host` is the eight-step move that turns an entity
//! this peer merely observed into one it owns: it rewrites the entity map, the
//! remote engine, the host engine, the redirect table and every pending
//! command. Each step is independently skippable without any of the others
//! noticing, and the failure mode is not a crash but a peer that afterwards
//! addresses the entity by an identifier the other side has retired.

use std::{collections::HashSet, net::SocketAddr};

use crate::{
    world::{
        component::property::Property, local::local_world_manager::LocalWorldManager,
        test_support::TestGwm,
    },
    BigMapKey, ComponentKind, ComponentKinds, EntityCommand, EntityMessageType, GlobalEntity,
    HostType, Instant, LocalEntityAndGlobalEntityConverter, OwnedLocalEntity, RemoteEntity,
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

fn global(id: u64) -> GlobalEntity {
    GlobalEntity::from_u64(id)
}

struct Fixture {
    kinds: ComponentKinds,
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
        Self { kinds, manager }
    }

    /// Registers `id` as a remote-owned entity carrying the given kinds.
    fn adopt_remote(&mut self, id: u64, kinds: Vec<ComponentKind>) -> RemoteEntity {
        let remote_entity = RemoteEntity::new(id as u32);
        self.manager.insert_remote_entity(
            &global(id),
            remote_entity,
            kinds.into_iter().collect::<HashSet<_>>(),
        );
        remote_entity
    }

    fn spawn_host(&mut self, id: u64) {
        let kinds = self.kinds.clone();
        self.manager.host_init_entity(
            &global(id),
            vec![ComponentKind::of::<Ghost>()],
            &kinds,
            false,
        );
        let _ = self.drain_commands();
    }

    /// The publish/delegation commands queued so far. The sender retains
    /// unacked commands, so the spawn from `spawn_host` keeps reappearing;
    /// only these two types are the subject here.
    fn delegation_commands(&mut self) -> Vec<EntityMessageType> {
        self.drain_commands()
            .iter()
            .map(EntityCommand::get_type)
            .filter(|kind| {
                matches!(
                    kind,
                    EntityMessageType::Publish | EntityMessageType::EnableDelegation
                )
            })
            .collect()
    }

    fn drain_commands(&mut self) -> Vec<EntityCommand> {
        let now = Instant::now();
        self.manager
            .take_outgoing_commands(&now, &200.0)
            .into_iter()
            .map(|(_, command)| command)
            .collect()
    }
}

// -- migration refusals ------------------------------------------------------

#[test]
fn an_entity_the_map_has_never_heard_of_cannot_migrate() {
    let mut fx = Fixture::new(HostType::Client);
    let error = fx
        .manager
        .migrate_entity_remote_to_host(&global(1))
        .expect_err("an unmapped entity must not migrate");
    assert!(
        error.contains("does not exist in local entity map"),
        "got {error:?}",
    );
}

/// The refusal path is the dangerous one: the validation *removes* the record
/// from the entity map before deciding, so a branch that returns without
/// restoring it deletes an entity this peer owns. Nothing else would report
/// that -- the entity simply stops resolving.
#[test]
fn refusing_to_migrate_a_host_owned_entity_puts_the_record_back() {
    let mut fx = Fixture::new(HostType::Server);
    fx.spawn_host(1);
    let before = fx
        .manager
        .entity_converter()
        .global_entity_to_host_entity(&global(1))
        .expect("fixture: the entity is host-owned");

    let error = fx
        .manager
        .migrate_entity_remote_to_host(&global(1))
        .expect_err("a host-owned entity is already where migration would put it");
    assert!(error.contains("not remote-owned"), "got {error:?}");

    assert_eq!(
        fx.manager
            .entity_converter()
            .global_entity_to_host_entity(&global(1))
            .expect("the record must survive the refusal"),
        before,
        "the refusal must restore the record it removed to inspect",
    );
}

// -- the successful migration ------------------------------------------------

/// Every step of the move, asserted separately. They are listed in the order
/// the method performs them, so a skipped step names itself.
#[test]
fn migrating_an_entity_moves_it_wholesale_from_the_remote_side_to_the_host_side() {
    let mut fx = Fixture::new(HostType::Client);
    let old_remote = fx.adopt_remote(1, vec![ComponentKind::of::<Ghost>()]);

    let new_host = fx
        .manager
        .migrate_entity_remote_to_host(&global(1))
        .expect("a remote-owned entity migrates");

    // The entity map now answers as host, and no longer as remote. The second
    // half matters most: a stale global->remote mapping is what lets a later
    // SetAuthority encode an identifier the peer has already retired.
    assert_eq!(
        fx.manager
            .entity_converter()
            .global_entity_to_host_entity(&global(1))
            .expect("the migrated entity is host-owned"),
        new_host,
    );
    assert!(
        fx.manager
            .entity_converter()
            .global_entity_to_remote_entity(&global(1))
            .is_err(),
        "the remote mapping must be gone, not merely shadowed",
    );

    // The host engine has a channel for the new id, carrying the component
    // state extracted from the remote channel -- not an empty one.
    let channel = fx
        .manager
        .get_host_entity_channel(&new_host)
        .expect("the new host entity must have a channel");
    assert!(
        channel
            .component_kinds()
            .contains(&ComponentKind::of::<Ghost>()),
        "the component state must survive the move",
    );

    // Old references resolve forward, so a message already in flight naming
    // the retired remote id still finds the entity.
    assert_eq!(
        fx.manager.apply_entity_redirect(old_remote.copy_to_owned()),
        OwnedLocalEntity::Host {
            id: new_host.value(),
            is_static: false,
        },
        "a redirect from the old remote id to the new host id must be installed",
    );

    assert!(
        !fx.manager.remote_entities().contains(&global(1)),
        "and the entity must no longer be listed as remote",
    );
}

/// A migrated entity keeps *all* of its components, not just the first. The
/// extraction step iterates, so a single-component fixture cannot tell the
/// difference between "extracted the set" and "extracted one".
#[test]
fn migration_carries_every_component_kind_across() {
    let mut fx = Fixture::new(HostType::Client);
    fx.adopt_remote(
        1,
        vec![ComponentKind::of::<Ghost>(), ComponentKind::of::<Wraith>()],
    );

    let new_host = fx
        .manager
        .migrate_entity_remote_to_host(&global(1))
        .expect("migrates");

    let channel = fx
        .manager
        .get_host_entity_channel(&new_host)
        .expect("channel");
    for kind in [ComponentKind::of::<Ghost>(), ComponentKind::of::<Wraith>()] {
        assert!(
            channel.component_kinds().contains(&kind),
            "{} must survive the migration",
            fx.kinds.kind_to_name(&kind),
        );
    }
}

/// Migrating twice must fail the second time rather than allocating a second
/// host id for the same entity -- the entity is host-owned by then, so it
/// takes the refusal path.
#[test]
fn an_entity_cannot_migrate_twice() {
    let mut fx = Fixture::new(HostType::Client);
    fx.adopt_remote(1, vec![ComponentKind::of::<Ghost>()]);
    let first = fx
        .manager
        .migrate_entity_remote_to_host(&global(1))
        .expect("migrates");

    let error = fx
        .manager
        .migrate_entity_remote_to_host(&global(1))
        .expect_err("the second migration must be refused");
    assert!(error.contains("not remote-owned"), "got {error:?}");
    assert_eq!(
        fx.manager
            .entity_converter()
            .global_entity_to_host_entity(&global(1))
            .expect("still mapped"),
        first,
        "and must not have allocated a second host id",
    );
}

// -- who may enable delegation, and how it is routed -------------------------

/// `send_enable_delegation` sorts on (host type, entity ownership, whether the
/// owning client originated it) and refuses four of the seven combinations.
/// Those refusals are the access-control rule for delegation: only the owning
/// client may delegate its own entity, and no client may delegate anyone
/// else's. Nothing else in the tree writes that rule down.
#[test]
fn only_the_owning_side_may_enable_delegation() {
    // (host type, entity is host-owned, origin is owning client, refusal)
    let refusals = [
        (
            HostType::Server,
            false,
            true,
            "Client cannot originate enable delegation for ANOTHER client-owned entity",
        ),
        (
            HostType::Client,
            true,
            false,
            "Client must be the owning client to enable delegation",
        ),
        (
            HostType::Client,
            false,
            false,
            "Client must be the owning client to enable delegation",
        ),
        (
            HostType::Client,
            false,
            true,
            "Client cannot enable delegation for a Server-owned entity",
        ),
    ];

    for (host_type, host_owned, origin_is_owning_client, expected) in refusals {
        let mut fx = Fixture::new(host_type);
        if host_owned {
            fx.spawn_host(1);
        } else {
            fx.adopt_remote(1, vec![ComponentKind::of::<Ghost>()]);
        }

        let message = panic_message_of(|| {
            fx.manager
                .send_enable_delegation(host_type, origin_is_owning_client, &global(1))
        });
        assert!(
            message.as_deref().is_some_and(|m| m.contains(expected)),
            "{host_type:?}/host_owned={host_owned}/origin={origin_is_owning_client} \
             must be refused with {expected:?}, got {message:?}",
        );
    }
}

#[test]
fn enabling_delegation_for_an_entity_that_does_not_exist_is_a_loud_failure() {
    let mut fx = Fixture::new(HostType::Server);
    let message = panic_message_of(|| {
        fx.manager
            .send_enable_delegation(HostType::Server, false, &global(1))
    });
    assert!(
        message
            .as_deref()
            .is_some_and(|m| m.contains("does not exist in local entity map")),
        "got {message:?}",
    );
}

/// A *client*-owned entity is published before it is delegated: the server
/// cannot delegate what it cannot see. Both commands go out, in that order.
#[test]
fn a_client_delegating_its_own_entity_publishes_it_first() {
    let mut fx = Fixture::new(HostType::Client);
    fx.spawn_host(1);

    fx.manager
        .send_enable_delegation(HostType::Client, true, &global(1));

    assert_eq!(
        fx.delegation_commands(),
        vec![
            EntityMessageType::Publish,
            EntityMessageType::EnableDelegation
        ],
    );
}

/// The server's own entities are published by definition, so delegating one
/// must *not* prepend a Publish -- the peer would receive a Publish for an
/// entity it has always been able to see.
#[test]
fn a_server_delegating_its_own_entity_does_not_publish_it_again() {
    let mut fx = Fixture::new(HostType::Server);
    fx.spawn_host(1);

    fx.manager
        .send_enable_delegation(HostType::Server, false, &global(1));

    assert_eq!(
        fx.delegation_commands(),
        vec![EntityMessageType::EnableDelegation],
        "server-owned entities are published by default",
    );
}

/// A server delegating a *client-owned* entity takes the other branch: it asks
/// the owner via the remote engine rather than telling a peer via the host one.
/// The two are distinguishable from outside because only the host branch
/// prepends a Publish -- and because the host branch would have to resolve a
/// host id this entity does not have.
#[test]
fn delegating_a_remote_owned_entity_asks_rather_than_publishes() {
    let mut fx = Fixture::new(HostType::Server);
    fx.adopt_remote(1, vec![ComponentKind::of::<Ghost>()]);

    fx.manager
        .send_enable_delegation(HostType::Server, false, &global(1));

    assert_eq!(
        fx.delegation_commands(),
        vec![EntityMessageType::EnableDelegation],
        "no Publish: the entity is not this peer's to publish",
    );
}

/// Returns the panic message if `body` panicked, silencing the default hook so
/// an expected panic does not spam the test output.
fn panic_message_of(body: impl FnOnce()) -> Option<String> {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
    std::panic::set_hook(previous);
    result.err().map(|payload| {
        payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
            .unwrap_or_else(|| "<non-string panic payload>".to_string())
    })
}
