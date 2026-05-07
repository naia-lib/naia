//! When-step bindings: delegation authority, scope include/exclude, and transport operations.

use naia_test_harness::EntityKey;

use crate::steps::prelude::*;
use crate::steps::vocab::ClientName;
use crate::steps::world_helpers::last_entity_mut;

// ──────────────────────────────────────────────────────────────────────
// Entity-delegation — server-side authority + scope ops
// ──────────────────────────────────────────────────────────────────────

/// When the server removes the delegated entity from client A's scope.
///
/// Excludes the entity from A's scope, triggering authority release
/// per [entity-delegation-13].
#[when("the server removes the delegated entity from client A's scope")]
fn when_server_removes_delegated_entity_from_client_a_scope(ctx: &mut TestWorldMut) {
    let client_a = named_client_mut(ctx, "A");
    let entity_key: EntityKey = ctx
        .scenario_mut()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no delegated entity spawned");
    ctx.scenario_mut().mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&client_a) {
                scope.exclude(&entity_key);
            }
        });
    });
}

/// When the server attempts to give authority to client {client} for the delegated entity.
///
/// Sibling of `when_server_gives_authority` that records the operation
/// result (Ok/Err/panic) into the scenario instead of unwrapping. Used
/// by `[common-01]` negative scenarios that assert `give_authority`
/// returns `Err` (e.g. `NotInScope`) rather than panicking.
#[when(
    "the server attempts to give authority to client {client} for the delegated entity"
)]
fn when_server_attempts_give_authority(ctx: &mut TestWorldMut, name: ClientName) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let client_key = named_client_mut(ctx, name.as_ref());
    let entity_key: EntityKey = ctx
        .scenario_mut()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no delegated entity spawned");
    let scenario = ctx.scenario_mut();
    scenario.clear_operation_result();
    // Capture (Result, panic-payload-if-any) inside a single mutate so the
    // borrow on `scenario` is released before recording.
    let captured: std::thread::Result<Result<(), String>> = {
        let mut out: std::thread::Result<Result<(), String>> = Ok(Ok(()));
        scenario.mutate(|mctx| {
            mctx.server(|server| {
                let res = catch_unwind(AssertUnwindSafe(|| {
                    if let Some(mut entity) = server.entity_mut(&entity_key) {
                        entity
                            .give_authority(&client_key)
                            .map(|_| ())
                            .map_err(|e| format!("{:?}", e))
                    } else {
                        Err("entity_mut returned None".to_string())
                    }
                }));
                out = res;
            });
        });
        out
    };
    match captured {
        Ok(Ok(())) => scenario.record_ok(),
        Ok(Err(e)) => scenario.record_err(e),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When the server gives authority to client {client} for the delegated entity.
#[when("the server gives authority to client {client} for the delegated entity")]
fn when_server_gives_authority(ctx: &mut TestWorldMut, name: ClientName) {
    let client_key = named_client_mut(ctx, name.as_ref());
    let entity_key: EntityKey = ctx
        .scenario_mut()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no delegated entity spawned");
    ctx.scenario_mut().mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity
                    .give_authority(&client_key)
                    .expect("give_authority should succeed for server");
            }
        });
    });
}

/// When the server takes authority for the delegated entity.
#[when("the server takes authority for the delegated entity")]
fn when_server_takes_authority(ctx: &mut TestWorldMut) {
    let entity_key = last_entity_mut(ctx);

    let scenario = ctx.scenario_mut();
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity
                    .take_authority()
                    .expect("take_authority should succeed for server");
            }
        });
    });
}

/// When the server releases authority for the delegated entity.
#[when("the server releases authority for the delegated entity")]
fn when_server_releases_authority(ctx: &mut TestWorldMut) {
    let entity_key = last_entity_mut(ctx);

    let scenario = ctx.scenario_mut();
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity
                    .release_authority()
                    .expect("release_authority should succeed for server");
            }
        });
    });
}

// ──────────────────────────────────────────────────────────────────────
// Entity-scope — server scope-mut operations
// ──────────────────────────────────────────────────────────────────────

/// When the server includes the entity for client {client}.
///
/// Named-client variant of `the server includes the entity for the client`.
/// Used by multi-client delegation tests where the act-on client is
/// distinct from the most-recently-connected one.
#[when("the server includes the entity for client {client}")]
fn when_server_includes_entity_for_named_client(ctx: &mut TestWorldMut, name: ClientName) {
    let client_key = named_client_mut(ctx, name.as_ref());
    let entity_key: EntityKey = ctx
        .scenario_mut()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");
    ctx.scenario_mut().mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&client_key) {
                scope.include(&entity_key);
            }
        });
    });
}

/// When the server excludes the entity for client {client}.
#[when("the server excludes the entity for client {client}")]
fn when_server_excludes_entity_for_named_client(ctx: &mut TestWorldMut, name: ClientName) {
    let client_key = named_client_mut(ctx, name.as_ref());
    let entity_key: EntityKey = ctx
        .scenario_mut()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");
    ctx.scenario_mut().mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&client_key) {
                scope.exclude(&entity_key);
            }
        });
    });
}

/// When the server configures the entity as Delegated.
///
/// Triggers `configure_replication(Delegated)` on the stored entity.
/// For client-owned entities this also runs the migration flow that
/// transfers ownership to the server (per [entity-ownership-11]).
#[when("the server configures the entity as Delegated")]
fn when_server_configures_entity_delegated(ctx: &mut TestWorldMut) {
    use naia_server::ReplicationConfig as ServerReplicationConfig;
    let entity_key = last_entity_mut(ctx);
    let scenario = ctx.scenario_mut();
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_key) {
                entity_mut.configure_replication(ServerReplicationConfig::delegated());
            }
        });
    });
}

/// When the server includes the entity for the client.
#[when("the server includes the entity for the client")]
fn when_server_includes_entity_for_client(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&client_key) {
                scope.include(&entity_key);
            }
        });
    });
    scenario.mutate(|_| {});
}

/// When the server excludes the entity for the client.
#[when("the server excludes the entity for the client")]
fn when_server_excludes_entity_for_client(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&client_key) {
                scope.exclude(&entity_key);
            }
        });
    });
    scenario.mutate(|_| {});
}

/// When the server includes an unknown entity for the client.
///
/// Edge-case test — invalid EntityKey should be a no-op.
#[when("the server includes an unknown entity for the client")]
fn when_server_includes_unknown_entity_for_client(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let unknown_entity_key = naia_test_harness::EntityKey::invalid();
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&client_key) {
                scope.include(&unknown_entity_key);
            }
        });
    });
    scenario.mutate(|_| {});
}

/// When the server includes the entity for an unknown client.
///
/// Edge-case test — invalid ClientKey should be a no-op
/// (`user_scope_mut` returns None for unknown clients).
#[when("the server includes the entity for an unknown client")]
fn when_server_includes_entity_for_unknown_client(ctx: &mut TestWorldMut) {
    let entity_key = last_entity_mut(ctx);

    let scenario = ctx.scenario_mut();
    let unknown_client_key = naia_test_harness::ClientKey::invalid();
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&unknown_client_key) {
                scope.include(&entity_key);
            }
        });
    });
    scenario.mutate(|_| {});
}

// ──────────────────────────────────────────────────────────────────────
// Transport — server outbound packet sends
// ──────────────────────────────────────────────────────────────────────

/// When the server sends a packet within the MTU limit.
#[when("the server sends a packet within the MTU limit")]
fn when_server_sends_packet_within_mtu(ctx: &mut TestWorldMut) {
    use naia_test_harness::test_protocol::{TestMessage, UnreliableChannel};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.send_message::<UnreliableChannel, _>(&client_key, &TestMessage::new(42));
        });
    });
    scenario.record_ok();
}

/// When the server attempts to send a packet exceeding MTU.
///
/// Catches any panic and records the outcome — the contract is that
/// oversized packets are rejected gracefully, not by panicking.
#[when("the server attempts to send a packet exceeding MTU")]
fn when_server_attempts_send_packet_exceeding_mtu(ctx: &mut TestWorldMut) {
    use naia_test_harness::test_protocol::{LargeTestMessage, UnreliableChannel};
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    let result = catch_unwind(AssertUnwindSafe(|| {
        scenario.mutate(|ctx| {
            ctx.server(|server| {
                server.send_message::<UnreliableChannel, _>(
                    &client_key,
                    &LargeTestMessage::new(1000),
                );
            });
        });
    }));
    match result {
        Ok(()) => scenario.record_err("Oversized packet rejected"),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When the server sends a large message on a reliable channel.
///
/// Sends a fragmentation-sized payload (5 000 B) on the ordered reliable
/// channel and catches any panic. The contract (messaging-16) is that
/// reliable channels MAY fragment; no panic is the success condition.
#[when("the server sends a large message on a reliable channel")]
fn when_server_sends_large_message_reliable(ctx: &mut TestWorldMut) {
    use naia_test_harness::test_protocol::{LargeTestMessage, ReliableChannel};
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    let result = catch_unwind(AssertUnwindSafe(|| {
        scenario.mutate(|ctx| {
            ctx.server(|server| {
                server.send_message::<ReliableChannel, _>(&client_key, &LargeTestMessage::new(5000));
            });
        });
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When the server mutates entity {label}'s component to x={int} y={int}.
///
/// `label` is "A" or "B"; resolves via [`entity_label_to_key_storage`].
/// Used by B-BDD-8 (per-entity convergence under cross-entity reorder).
#[when("the server mutates entity {word}'s component to x={int} y={int}")]
fn when_server_mutates_entity_component(
    ctx: &mut TestWorldMut,
    label: String,
    x: i32,
    y: i32,
) {
    use naia_test_harness::Position;
    let scenario = ctx.scenario_mut();
    let entity_key: EntityKey = scenario
        .bdd_get(entity_label_to_key_storage(&label))
        .unwrap_or_else(|| panic!("entity '{}' not stored", label));
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                if let Some(mut pos) = entity.component::<Position>() {
                    *pos.x = x as f32;
                    *pos.y = y as f32;
                }
            }
        });
    });
}
