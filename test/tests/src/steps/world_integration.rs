//! Step bindings for World Integration contract (14_world_integration.feature)
//!
//! These steps cover:
//!   - Entity presence in client world mirrors scope (world-integration-04)
//!   - Late-joining client receives current server snapshot (world-integration-05)

use std::time::Duration;

use naia_client::{ClientConfig, JitterBufferType};
use naia_test_harness::{
    protocol, Auth, ClientConnectEvent, ClientKey, EntityKey, Position, ServerAuthEvent,
    ServerConnectEvent, TrackedClientEvent, TrackedServerEvent,
};
use namako_engine::codegen::AssertOutcome;
use namako_engine::{then, when};

use crate::{TestWorldMut, TestWorldRef};

const LAST_ENTITY_KEY: &str = "last_entity";

// ============================================================================
// When Steps — Late Join
// ============================================================================

/// Step: When a second client connects and the entity enters scope for it
///
/// Connects a second client through the full handshake flow, adds it to the
/// room, and includes the last entity in its scope. The "second client" is
/// stored in bdd storage as "second_client".
#[when("a second client connects and the entity enters scope for it")]
fn when_second_client_connects_and_entity_enters_scope(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();

    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned for world integration test");

    let test_protocol = protocol();
    let room_key = scenario.last_room();

    let mut client_config = ClientConfig::default();
    client_config.send_handshake_interval = Duration::from_millis(0);
    client_config.jitter_buffer = JitterBufferType::Bypass;

    let client_key = scenario.client_start(
        "SecondClient",
        Auth::new("second_client", "password"),
        client_config,
        test_protocol,
    );

    scenario.expect(|ectx| {
        ectx.server(|server| {
            if let Some((incoming_key, _auth)) = server.read_event::<ServerAuthEvent<Auth>>() {
                if incoming_key == client_key {
                    return Some(incoming_key);
                }
            }
            None
        })
    });

    scenario.mutate(|mctx| {
        mctx.server(|server| {
            server.accept_connection(&client_key);
        });
    });

    scenario.expect(|ectx| {
        ectx.server(|server| {
            if let Some(incoming_key) = server.read_event::<ServerConnectEvent>() {
                if incoming_key == client_key {
                    return Some(());
                }
            }
            None
        })
    });
    scenario.track_server_event(TrackedServerEvent::Connect);

    scenario.mutate(|mctx| {
        mctx.server(|server| {
            server
                .room_mut(&room_key)
                .expect("room exists")
                .add_user(&client_key);
            if let Some(mut scope) = server.user_scope_mut(&client_key) {
                scope.include(&entity_key);
            }
        });
    });

    scenario.expect(|ectx| {
        ectx.client(client_key, |client| {
            client.read_event::<ClientConnectEvent>()
        })
    });
    scenario.track_client_event(client_key, TrackedClientEvent::Connect);

    scenario.bdd_store("second_client", client_key);
    scenario.allow_flexible_next();
}

// ============================================================================
// Then Steps — World Mirror Assertions
// ============================================================================

/// Step: When the server removes the replicated component
///
/// Removes the Position component from the last entity server-side.
/// This exercises the component removal replication path.
#[when("the server removes the replicated component")]
fn when_server_removes_replicated_component(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");

    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity.remove_component::<Position>();
            }
        });
    });
}

/// Step: Then the client world has the component on the entity
///
/// Polls until the client's local entity has the Position component.
/// Covers [world-integration-08.t1]: component insert propagates to client world
/// such that component sets MUST match after mutations are applied.
#[then("the client world has the component on the entity")]
fn then_client_world_has_component_on_entity(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");

    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            if entity.has_component::<Position>() {
                AssertOutcome::Passed(())
            } else {
                AssertOutcome::Pending
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: Then the client world no longer has the component on the entity
///
/// Polls until the client's local entity no longer has the Position component.
/// Covers [world-integration-09.t1]: component removal propagates to client world
/// such that component values MUST match after mutations are applied.
#[then("the client world no longer has the component on the entity")]
fn then_client_world_no_longer_has_component(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");

    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            if entity.component::<Position>().is_none() {
                AssertOutcome::Passed(())
            } else {
                AssertOutcome::Pending
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: Then the second client has the entity in its world
///
/// Polls until the second client has the entity replicated locally.
/// Covers [world-integration-05.t1]: late-joining client receives current snapshot.
#[then("the second client has the entity in its world")]
fn then_second_client_has_entity_in_world(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let second_client: ClientKey = ctx
        .scenario()
        .bdd_get("second_client")
        .expect("second client not connected");
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");

    ctx.client(second_client, |c| {
        if c.has_entity(&entity_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}
