//! Step bindings for Entity Publication contract (09_entity_publication.feature)
//!
//! These steps cover:
//!   - Named client connections (client A, client B, etc.)
//!   - Client-owned entity spawning with Private/Public replication config
//!   - Room sharing between named clients and entities
//!   - Scope assertions for named clients

use std::time::Duration;

use namako_engine::{given, then};
use naia_test_harness::{
    protocol, Auth, EntityKey, Position,
    ServerAuthEvent, ServerConnectEvent,
    TrackedServerEvent, TrackedClientEvent,
    ClientConnectEvent, ClientKey,
};
use naia_client::{ClientConfig, JitterBufferType, ReplicationConfig as ClientReplicationConfig};

use crate::{TestWorldMut, TestWorldRef};

/// Storage key for the last entity created in BDD tests
const LAST_ENTITY_KEY: &str = "last_entity";

/// Storage key prefix for named clients (e.g., "client_A", "client_B")
fn client_key_storage(name: &str) -> String {
    format!("client_{}", name)
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Connect a named client to the scenario.
fn connect_named_client(ctx: &mut TestWorldMut, name: &str) {
    let scenario = ctx.scenario_mut();
    let test_protocol = protocol();
    let room_key = scenario.last_room();

    // Configure client for immediate handshake
    let mut client_config = ClientConfig::default();
    client_config.send_handshake_interval = Duration::from_millis(0);
    client_config.jitter_buffer = JitterBufferType::Bypass;

    let client_key = scenario.client_start(
        &format!("Client {}", name),
        Auth::new(&format!("client_{}", name.to_lowercase()), "password"),
        client_config,
        test_protocol,
    );

    // Wait for auth event and accept connection
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

    // Wait for server connect event
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

    // Add client to room
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            server.room_mut(&room_key).expect("room exists").add_user(&client_key);
        });
    });

    // Wait for client connect event
    scenario.expect(|ectx| {
        ectx.client(client_key, |client| {
            client.read_event::<ClientConnectEvent>()
        })
    });
    scenario.track_client_event(client_key, TrackedClientEvent::Connect);

    // Store the named client key for later retrieval
    scenario.bdd_store(&client_key_storage(name), client_key);

    scenario.allow_flexible_next();
}

/// Get a named client's key from storage (mutable context version).
fn get_named_client_mut(ctx: &mut TestWorldMut, name: &str) -> ClientKey {
    ctx.scenario_mut()
        .bdd_get(&client_key_storage(name))
        .unwrap_or_else(|| panic!("No client '{}' has been connected", name))
}

// ============================================================================
// Given Steps - Named Client Connection
// ============================================================================

/// Step: Given client {word} connects
/// Connects a named client (A, B, C, etc.) to the server.
#[given("client {word} connects")]
fn given_client_named_connects(ctx: &mut TestWorldMut, name: String) {
    connect_named_client(ctx, &name);
}

// ============================================================================
// Given Steps - Entity Spawning with Replication Config
// ============================================================================

/// Step: Given client {word} spawns a client-owned entity with Private replication config
/// Client spawns an entity with Private (unpublished) replication config.
#[given("client {word} spawns a client-owned entity with Private replication config")]
fn given_client_spawns_entity_private(ctx: &mut TestWorldMut, name: String) {
    let client_key = get_named_client_mut(ctx, &name);
    let scenario = ctx.scenario_mut();

    // Client spawns entity with Private replication
    let entity_key = scenario.mutate(|mctx| {
        mctx.client(client_key, |client| {
            client.spawn(|mut entity| {
                entity
                    .configure_replication(ClientReplicationConfig::Private)
                    .insert_component(Position::new(0.0, 0.0));
            })
        })
    });

    // Wait for entity to replicate to server
    scenario.expect(|ectx| {
        ectx.server(|server| {
            if server.has_entity(&entity_key) {
                Some(())
            } else {
                None
            }
        })
    });

    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
    scenario.allow_flexible_next();
}

/// Step: Given client {word} spawns a client-owned entity with Public replication config
/// Client spawns an entity with Public (published) replication config.
#[given("client {word} spawns a client-owned entity with Public replication config")]
fn given_client_spawns_entity_public(ctx: &mut TestWorldMut, name: String) {
    let client_key = get_named_client_mut(ctx, &name);
    let scenario = ctx.scenario_mut();

    // Client spawns entity with Public replication
    let entity_key = scenario.mutate(|mctx| {
        mctx.client(client_key, |client| {
            client.spawn(|mut entity| {
                entity
                    .configure_replication(ClientReplicationConfig::Public)
                    .insert_component(Position::new(0.0, 0.0));
            })
        })
    });

    // Wait for entity to replicate to server
    scenario.expect(|ectx| {
        ectx.server(|server| {
            if server.has_entity(&entity_key) {
                Some(())
            } else {
                None
            }
        })
    });

    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
    scenario.allow_flexible_next();
}

// ============================================================================
// Given Steps - Room Sharing
// ============================================================================

/// Step: Given client {word} and the entity share a room
/// Adds the entity to the room that the named client is in.
#[given("client {word} and the entity share a room")]
fn given_client_and_entity_share_room(ctx: &mut TestWorldMut, name: String) {
    let client_key = get_named_client_mut(ctx, &name);
    let scenario = ctx.scenario_mut();
    let room_key = scenario.last_room();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity.enter_room(&room_key);
            }
            // Also include entity in client's scope
            if let Some(mut scope) = server.user_scope_mut(&client_key) {
                scope.include(&entity_key);
            }
        });
    });

    // Advance a tick to let changes propagate
    scenario.mutate(|_| {});
}

// ============================================================================
// Then Steps - Scope Assertions for Named Clients
// ============================================================================

/// Helper function to check if entity is in scope for a named client.
fn check_entity_in_scope(ctx: &TestWorldRef, client_name: &str) -> bool {
    let client_key: ClientKey = ctx
        .scenario()
        .bdd_get(&client_key_storage(client_name))
        .unwrap_or_else(|| panic!("No client '{}' has been connected", client_name));
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    ctx.server(|server| {
        if let Some(scope) = server.user_scope(&client_key) {
            scope.has(&entity_key)
        } else {
            false
        }
    })
}

/// Step: Then the entity is in-scope for client A
/// Verifies the entity is in client A's scope.
#[then("the entity is in-scope for client A")]
fn then_entity_in_scope_for_client_a(ctx: &TestWorldRef) {
    assert!(
        check_entity_in_scope(ctx, "A"),
        "Expected entity to be in-scope for client A, but it was not"
    );
}

/// Step: Then the entity is in-scope for client B
/// Verifies the entity is in client B's scope.
#[then("the entity is in-scope for client B")]
fn then_entity_in_scope_for_client_b(ctx: &TestWorldRef) {
    assert!(
        check_entity_in_scope(ctx, "B"),
        "Expected entity to be in-scope for client B, but it was not"
    );
}

/// Step: Then the entity is out-of-scope for client B
/// Verifies the entity is not in client B's scope.
#[then("the entity is out-of-scope for client B")]
fn then_entity_out_of_scope_for_client_b(ctx: &TestWorldRef) {
    assert!(
        !check_entity_in_scope(ctx, "B"),
        "Expected entity to be out-of-scope for client B, but it was in-scope"
    );
}
