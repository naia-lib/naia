//! High-level connect-handshake, entity-spawn, and assertion helpers.
//!
//! Builds on the low-level primitives in [`world_helpers`](super::world_helpers).
//! These helpers are larger or involve multi-step sequences, so they live here
//! to keep `world_helpers` under 500 LOC.

use std::time::Duration;

use naia_client::{ClientConfig, JitterBufferType};
use naia_server::ServerConfig;
use naia_test_harness::{
    protocol, Auth, ClientConnectEvent, ServerAuthEvent, ServerConnectEvent, TrackedClientEvent,
    TrackedServerEvent,
};

use crate::{TestWorldMut, TestWorldRef};
use crate::steps::world_helpers::{client_key_storage, LAST_ENTITY_KEY};

/// Connect a test client by short label ("A", "B", ...).
///
/// Creates a client with name `"Client {label}"`, auth username
/// `"client_{label_lowercase}"`, and stores the resulting `ClientKey`
/// under `client_key_storage(label)` for downstream lookup.
///
/// Used by multi-client tests (entity-publication, entity-authority,
/// scope-propagation, etc.) where bindings reference clients by
/// label rather than the singleton "last client".
///
/// # Example
/// ```ignore
/// #[given("client {word} connects")]
/// fn given_client_named_connects(ctx: &mut TestWorldMut, name: String) {
///     connect_test_client(ctx, &name);
/// }
/// ```
pub fn connect_test_client(ctx: &mut TestWorldMut, label: &str) -> crate::ClientKey {
    let client_key = connect_named_client(
        ctx,
        &format!("Client {}", label),
        &format!("client_{}", label.to_lowercase()),
        None,
    );
    ctx.scenario_mut()
        .bdd_store(&client_key_storage(label), client_key);
    client_key
}

/// Like [`connect_named_client`], but also tracks the server-side
/// `ServerAuthEvent`. Used by tests that assert the auth → connect
/// event ordering.
pub fn connect_named_client_with_auth_tracking(
    ctx: &mut TestWorldMut,
    client_name: &str,
    username: &str,
) -> crate::ClientKey {
    let scenario = ctx.scenario_mut();
    let test_protocol = protocol();
    let room_key = scenario.last_room();

    let client_config = ClientConfig {
        send_handshake_interval: Duration::from_millis(0),
        jitter_buffer: JitterBufferType::Bypass,
        ..Default::default()
    };

    let client_key = scenario.client_start(
        client_name,
        Auth::new(username, "password"),
        client_config,
        test_protocol,
    );

    expect_server_auth_for(scenario, client_key);
    scenario.track_server_event(TrackedServerEvent::Auth);
    accept_client(scenario, client_key);
    expect_server_connect_for(scenario, client_key);
    scenario.track_server_event(TrackedServerEvent::Connect);
    scenario.mutate(|_| {});
    expect_client_connect(scenario, client_key);
    scenario.track_client_event(client_key, TrackedClientEvent::Connect);
    add_user_to_room(scenario, client_key, room_key);
    scenario.allow_flexible_next();
    client_key
}

/// Drive the auth flow + server-side reject, tracking the client's
/// `ClientRejectEvent` so downstream Then steps can assert it.
pub fn reject_named_client(
    ctx: &mut TestWorldMut,
    client_name: &str,
    username: &str,
) -> crate::ClientKey {
    use naia_test_harness::ClientRejectEvent;
    let scenario = ctx.scenario_mut();
    let test_protocol = protocol();

    let client_config = ClientConfig {
        send_handshake_interval: Duration::from_millis(0),
        jitter_buffer: JitterBufferType::Bypass,
        ..Default::default()
    };

    let client_key = scenario.client_start(
        client_name,
        Auth::new(username, "password"),
        client_config,
        test_protocol,
    );
    expect_server_auth_for(scenario, client_key);
    scenario.mutate(|c| c.server(|s| s.reject_connection(&client_key)));
    scenario.expect(|c| {
        c.client(client_key, |client| client.read_event::<ClientRejectEvent>())
    });
    scenario.track_client_event(client_key, TrackedClientEvent::Reject);
    scenario.allow_flexible_next();
    client_key
}

// ──────────────────────────────────────────────────────────────────────
// Connect-handshake primitives — small bricks for custom flows.
// ──────────────────────────────────────────────────────────────────────

pub fn expect_server_auth_for_key(scenario: &mut crate::Scenario, client_key: crate::ClientKey) {
    expect_server_auth_for(scenario, client_key);
}

fn expect_server_auth_for(scenario: &mut crate::Scenario, client_key: crate::ClientKey) {
    scenario.expect(|c| {
        c.server(|s| {
            s.read_event::<ServerAuthEvent<Auth>>()
                .filter(|(k, _)| *k == client_key)
                .map(|(k, _)| k)
        })
    });
}

fn expect_server_connect_for(scenario: &mut crate::Scenario, client_key: crate::ClientKey) {
    scenario.expect(|c| {
        c.server(|s| {
            s.read_event::<ServerConnectEvent>()
                .filter(|k| *k == client_key)
                .map(|_| ())
        })
    });
}

fn expect_client_connect(scenario: &mut crate::Scenario, client_key: crate::ClientKey) {
    scenario.expect(|c| {
        c.client(client_key, |client| client.read_event::<ClientConnectEvent>())
    });
}

fn accept_client(scenario: &mut crate::Scenario, client_key: crate::ClientKey) {
    scenario.mutate(|c| c.server(|s| s.accept_connection(&client_key)));
}

fn add_user_to_room(
    scenario: &mut crate::Scenario,
    client_key: crate::ClientKey,
    room_key: crate::RoomKey,
) {
    scenario.mutate(|c| {
        c.server(|s| {
            s.room_mut(&room_key)
                .expect("room exists")
                .add_user(&client_key);
        });
    });
}

/// Spawn a delegated server-owned entity with `Position::new(0,0)` in
/// `last_room` and include it in each of `clients`' scopes. Stores
/// the resulting `EntityKey` in `LAST_ENTITY_KEY`. Returns the key.
pub fn spawn_delegated_entity_in_scope(
    ctx: &mut TestWorldMut,
    clients: &[crate::ClientKey],
) -> naia_test_harness::EntityKey {
    use naia_server::ReplicationConfig as SRC;
    use naia_test_harness::Position;
    let scenario = ctx.scenario_mut();
    let room_key = scenario.last_room();
    let (entity_key, ()) = scenario.mutate(|c| c.server(|s|
        s.spawn(|mut e| { e.insert_component(Position::new(0.0, 0.0))
            .configure_replication(SRC::delegated()).enter_room(&room_key); })));
    let clients_v: Vec<crate::ClientKey> = clients.to_vec();
    scenario.mutate(|c| c.server(|s| {
        for ck in &clients_v {
            if let Some(mut scope) = s.user_scope_mut(ck) { scope.include(&entity_key); }
        }
    }));
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
    entity_key
}

/// Spawn a server-owned entity with a `Position::new(0,0)` in
/// `last_room` and include it in `client_key`'s scope. Stores the
/// resulting `EntityKey` in `LAST_ENTITY_KEY`. Returns the key.
pub fn spawn_position_entity_in_scope(
    ctx: &mut TestWorldMut,
    client_key: crate::ClientKey,
) -> naia_test_harness::EntityKey {
    use naia_test_harness::Position;
    let scenario = ctx.scenario_mut();
    let room_key = scenario.last_room();
    let (entity_key, ()) = scenario.mutate(|c| {
        c.server(|s| s.spawn(|mut e| {
            e.insert_component(Position::new(0.0, 0.0)).enter_room(&room_key);
        }))
    });
    scenario.mutate(|c| c.server(|s| {
        if let Some(mut scope) = s.user_scope_mut(&client_key) { scope.include(&entity_key); }
    }));
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
    entity_key
}

/// Assert (with `AssertOutcome::Pending` polling) that the **server**
/// view of `entity_key` has Position equal to `expected`. Fall-through
/// cases (entity missing, component missing, value not yet equal) all
/// return Pending — the BDD `expect` loop polls until it passes or
/// times out.
pub fn assert_server_position_eq(
    ctx: &TestWorldRef,
    entity_key: naia_test_harness::EntityKey,
    expected: (f32, f32),
) -> namako_engine::codegen::AssertOutcome<()> {
    use namako_engine::codegen::AssertOutcome;
    use naia_test_harness::Position;
    ctx.server(|server| {
        let Some(entity) = server.entity(&entity_key) else { return AssertOutcome::Pending; };
        let Some(pos) = entity.component::<Position>() else { return AssertOutcome::Pending; };
        if (*pos.x - expected.0).abs() < f32::EPSILON && (*pos.y - expected.1).abs() < f32::EPSILON {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Same as [`assert_server_position_eq`] but for the **client** view.
pub fn assert_client_position_eq(
    ctx: &TestWorldRef,
    client_key: crate::ClientKey,
    entity_key: naia_test_harness::EntityKey,
    expected: (f32, f32),
) -> namako_engine::codegen::AssertOutcome<()> {
    use namako_engine::codegen::AssertOutcome;
    use naia_test_harness::Position;
    ctx.client(client_key, |client| {
        let Some(entity) = client.entity(&entity_key) else { return AssertOutcome::Pending; };
        let Some(pos) = entity.component::<Position>() else { return AssertOutcome::Pending; };
        if (*pos.x - expected.0).abs() < f32::EPSILON && (*pos.y - expected.1).abs() < f32::EPSILON {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Idempotently start the server. If the scenario isn't initialized
/// yet, init it, start the server with default config, create a
/// default room, and store it as `last_room`. If it's already
/// initialized, no-op.
///
/// Used by feature files that may be authored either with or without
/// an explicit `Given a server is running` precondition (the
/// replicated-resources scenarios use the implicit form).
pub fn ensure_server_started(ctx: &mut TestWorldMut) {
    if ctx.is_initialized() {
        return;
    }
    let scenario = ctx.init();
    scenario.server_start(ServerConfig::default(), protocol());
    let room_key = scenario.mutate(|c| c.server(|server| server.create_room().key()));
    scenario.set_last_room(room_key);
}

/// Run the standard client-connect handshake for the next test client.
///
/// The handshake is identical for every test that needs a connected
/// client: build a `ClientConfig` with bypass jitter buffer + zero
/// handshake interval, queue auth, accept on server, add to the
/// scenario's `last_room`, then wait for both server-side
/// `ServerConnectEvent` and client-side `ClientConnectEvent` and
/// track them. The bound step binding becomes a one-liner over this.
///
/// # Example
/// ```ignore
/// #[given("a client connects")]
/// fn given_client_connects(ctx: &mut TestWorldMut) {
///     connect_client(ctx);
/// }
/// ```
pub fn connect_client(ctx: &mut TestWorldMut) {
    connect_named_client(ctx, "TestClient", "test_user", None);
}

/// Connect a client with an explicit name + username. Optional
/// `extra_setup` runs after the room-add but before the
/// `ClientConnectEvent` wait — typically used by world-integration
/// tests to also include a specific entity in the new client's scope.
///
/// # Example
/// ```ignore
/// connect_named_client(ctx, "SecondClient", "second_client", Some(Box::new(move |scenario, client_key| {
///     scenario.mutate(|m| m.server(|s| {
///         if let Some(mut scope) = s.user_scope_mut(&client_key) {
///             scope.include(&entity_key);
///         }
///     }));
/// })));
/// ```
#[allow(clippy::type_complexity)]
pub fn connect_named_client(
    ctx: &mut TestWorldMut,
    client_name: &str,
    username: &str,
    extra_setup: Option<Box<dyn FnOnce(&mut crate::Scenario, crate::ClientKey)>>,
) -> crate::ClientKey {
    let scenario = ctx.scenario_mut();
    let test_protocol = protocol();
    let room_key = scenario.last_room();

    let client_config = ClientConfig {
        send_handshake_interval: Duration::from_millis(0),
        jitter_buffer: JitterBufferType::Bypass,
        ..Default::default()
    };

    let client_key = scenario.client_start(
        client_name,
        Auth::new(username, "password"),
        client_config,
        test_protocol,
    );

    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((incoming_key, _auth)) = server.read_event::<ServerAuthEvent<Auth>>() {
                if incoming_key == client_key {
                    return Some(incoming_key);
                }
            }
            None
        })
    });
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.accept_connection(&client_key);
        });
    });

    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some(incoming_key) = server.read_event::<ServerConnectEvent>() {
                if incoming_key == client_key {
                    return Some(());
                }
            }
            None
        })
    });
    scenario.track_server_event(TrackedServerEvent::Connect);

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .room_mut(&room_key)
                .expect("room exists")
                .add_user(&client_key);
        });
    });

    if let Some(setup) = extra_setup {
        setup(scenario, client_key);
    }

    scenario.expect(|ctx| {
        ctx.client(client_key, |client| client.read_event::<ClientConnectEvent>())
    });
    scenario.track_client_event(client_key, TrackedClientEvent::Connect);

    scenario.allow_flexible_next();
    client_key
}

/// Connect a latency-conditioned client by label.
///
/// Runs the full auth → accept → ServerConnect → room-add → ClientConnect
/// handshake, then applies a symmetric `LinkConditionerConfig(latency_ms, 0, 0.0)`.
/// The `label` becomes the display name and is stored under `client_key_storage(label)`
/// for downstream `named_client_*` lookups.
///
/// Used by the "Given/When a client connects with latency {int}ms" bindings, which
/// differ only by label ("LatencyClient" vs "ReconnectedClient").
pub fn connect_client_with_latency(
    ctx: &mut crate::TestWorldMut,
    label: &str,
    latency_ms: u32,
) -> crate::ClientKey {
    use std::time::Duration;
    use naia_client::{ClientConfig, JitterBufferType};
    use naia_test_harness::{
        Auth, ClientConnectEvent, LinkConditionerConfig, ServerAuthEvent, ServerConnectEvent,
    };
    let scenario = ctx.scenario_mut();
    let test_protocol = protocol();
    let room_key = scenario.last_room();
    let client_config = ClientConfig {
        send_handshake_interval: Duration::from_millis(0),
        jitter_buffer: JitterBufferType::Bypass,
        ..Default::default()
    };
    let client_key = scenario.client_start(
        label,
        Auth::new("test_user", "password"),
        client_config,
        test_protocol,
    );
    let latency_config = LinkConditionerConfig::new(latency_ms, 0, 0.0);
    scenario.configure_link_conditioner(
        &client_key,
        Some(latency_config.clone()),
        Some(latency_config),
    );
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((incoming_key, _auth)) = server.read_event::<ServerAuthEvent<Auth>>() {
                if incoming_key == client_key { return Some(incoming_key); }
            }
            None
        })
    });
    scenario.mutate(|ctx| { ctx.server(|server| { server.accept_connection(&client_key); }); });
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some(incoming_key) = server.read_event::<ServerConnectEvent>() {
                if incoming_key == client_key { return Some(()); }
            }
            None
        })
    });
    scenario.track_server_event(TrackedServerEvent::Connect);
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.room_mut(&room_key).expect("room exists").add_user(&client_key);
        });
    });
    scenario.expect(|ctx| {
        ctx.client(client_key, |client| client.read_event::<ClientConnectEvent>())
    });
    scenario.track_client_event(client_key, TrackedClientEvent::Connect);
    scenario.allow_flexible_next();
    scenario.bdd_store(&client_key_storage(label), client_key);
    client_key
}
