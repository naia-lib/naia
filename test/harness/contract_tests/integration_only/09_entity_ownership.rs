#![allow(unused_imports, unused_variables, unused_must_use, unused_mut, dead_code, for_loops_over_fallibles)]

use std::time::Duration;

use naia_client::{ClientConfig, JitterBufferType, ReplicationConfig as ClientReplicationConfig};
use naia_server::{ReplicationConfig, RoomKey, ServerConfig};
use naia_shared::{Protocol, Tick};

use naia_test_harness::{
    protocol, Auth, ClientConnectEvent, ClientDespawnEntityEvent, ClientDisconnectEvent,
    ClientKey, ClientRejectEvent, EntityOwner, ExpectCtx, Position, Scenario,
    ServerAuthEvent, ServerConnectEvent,
    ToTicks,
};

mod _helpers;
use _helpers::{client_connect, server_and_client_connected, test_client_config};


// ============================================================================
// Entity Ownership Tests
// ============================================================================
// Tests organized by contract ID to match specs/contracts/08_entity_ownership.spec.md
// ============================================================================

/// Enabling delegation on client-owned entity migrates ownership to server
/// Contract: [entity-ownership-11]
///
/// t1: Enabling delegation transfers ownership from Client(A) → Server.
/// t2: After migration, configure_replication back to Public keeps server ownership —
///     entity cannot revert to client ownership.
#[test]
fn client_owned_entity_migrates_to_server_owned_delegated() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "pass"),
        test_client_config(),
        test_protocol,
    );

    // Client A spawns Published entity (Public = visible to other clients when in-scope)
    let entity_e = scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            client_a.spawn(|mut e| {
                e.configure_replication(ClientReplicationConfig::Public)
                    .insert_component(Position::new(1.0, 2.0));
            })
        })
    });

    // Wait for entity to replicate to server
    scenario.expect(|ctx| ctx.server(|server| server.has_entity(&entity_e).then_some(())));

    // t1: Before delegation, owner is Client(A) on server side
    scenario.expect(|ctx| {
        let owner = ctx.server(|server| {
            server.entity(&entity_e).map(|e| e.owner())
        });
        (owner == Some(EntityOwner::Client(client_a_key))).then_some(())
    });

    // Server enables delegation — this migrates ownership from Client(A) → Server
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.configure_replication(ReplicationConfig::delegated());
            }
        });
    });

    // t1: After delegation, owner is Server (not Client(A))
    scenario.expect(|ctx| {
        let owner = ctx.server(|server| {
            server.entity(&entity_e).map(|e| e.owner())
        });
        let config = ctx.server(|server| {
            server.entity(&entity_e).and_then(|e| e.replication_config())
        });
        let owner_is_server = owner == Some(EntityOwner::Server);
        let config_is_delegated = config == Some(ReplicationConfig::delegated());
        (owner_is_server && config_is_delegated).then_some(())
    });

    // t2: Reconfigure to Public — server still owns it, cannot revert to client ownership
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.configure_replication(ReplicationConfig::public());
            }
        });
    });

    // t2: Owner remains Server even after reconfigure to Public
    scenario.expect(|ctx| {
        let owner = ctx.server(|server| {
            server.entity(&entity_e).map(|e| e.owner())
        });
        let config = ctx.server(|server| {
            server.entity(&entity_e).and_then(|e| e.replication_config())
        });
        let owner_still_server = owner == Some(EntityOwner::Server);
        let config_is_public = config == Some(ReplicationConfig::public());
        (owner_still_server && config_is_public).then_some(())
    });
}

/// Private entity is never visible to non-owner and owner retains it across scope changes
/// Contract: [entity-ownership-12] t1
///
/// A spawns a Private (unpublished) entity. B cannot see it regardless of scope manipulation.
/// The owning client A retains the entity and never receives a despawn event for it.
#[test]
fn private_entity_owner_retains_across_scope_changes() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "pass"),
        test_client_config(),
        test_protocol.clone(),
    );
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "pass"),
        test_client_config(),
        test_protocol,
    );

    // Client A spawns Private entity — only A sees it; server cannot replicate to others
    let entity_e = scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            client_a.spawn(|mut e| {
                e.configure_replication(ClientReplicationConfig::Private)
                    .insert_component(Position::new(1.0, 2.0));
            })
        })
    });

    // Wait for entity to appear on A and replicate to server
    scenario.expect(|ctx| {
        let a_has = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let server_has = ctx.server(|server| server.has_entity(&entity_e));
        (a_has && server_has).then_some(())
    });

    // B must not see the Private entity
    // Private entities cannot be replicated to non-owner clients
    let b_has_entity = scenario.mutate(|ctx| {
        ctx.client(client_b_key, |b| b.entity(&entity_e).is_some())
    });
    assert!(!b_has_entity, "Non-owner B must not see Private entity");

    // Server attempts to include B in scope for entity_e — must be a no-op for Private entities
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.enter_room(&room_key);
            }
            server.user_scope_mut(&client_b_key).unwrap().include(&entity_e);
        });
    });

    // Allow multiple ticks for any (erroneous) replication to propagate
    scenario.expect(|ctx| {
        // A still has entity and B still does not
        let a_has = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_has = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        // A owns it (client-owned, so owner = Client on A's side)
        let a_owns = ctx.client(client_a_key, |c| {
            c.entity(&entity_e)
                .map(|e| e.owner() == EntityOwner::Client(client_a_key))
                .unwrap_or(false)
        });
        (a_has && !b_has && a_owns).then_some(())
    });

    // A must never have received a despawn event for entity_e throughout this sequence
    // If the server had erroneously sent a despawn to A, has_entity would be false above.
    // Explicitly verify no despawn arrived for A in this tick.
    scenario.expect(|ctx| {
        let a_no_despawn = !ctx.client(client_a_key, |c| c.has::<ClientDespawnEntityEvent>());
        let b_no_despawn = !ctx.client(client_b_key, |c| c.has::<ClientDespawnEntityEvent>());
        (a_no_despawn && b_no_despawn).then_some(())
    });
}

/// Non-owner despawns entity on scope exit; owner does not
/// Contract: [entity-ownership-12] t1+t2
///
/// A spawns a Public (published) entity visible to B. When B leaves scope, B receives
/// a despawn event. A — as the owner — does NOT receive a despawn and retains the entity.
#[test]
fn owner_retains_entity_when_non_owner_leaves_scope() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "pass"),
        test_client_config(),
        test_protocol.clone(),
    );
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "pass"),
        test_client_config(),
        test_protocol,
    );

    // Client A spawns Public entity — visible to B when in-scope
    let entity_e = scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            client_a.spawn(|mut e| {
                e.configure_replication(ClientReplicationConfig::Public)
                    .insert_component(Position::new(5.0, 5.0));
            })
        })
    });

    // Wait for server to see entity
    scenario.expect(|ctx| ctx.server(|server| server.has_entity(&entity_e).then_some(())));

    scenario.allow_flexible_next();

    // Put entity in room and include B in scope so B can see it
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.enter_room(&room_key);
            }
            server.user_scope_mut(&client_b_key).unwrap().include(&entity_e);
        });
    });

    // Wait for both A and B to have the entity
    scenario.expect(|ctx| {
        let a_has = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_has = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        (a_has && b_has).then_some(())
    });

    // Server excludes B from scope — B should lose (despawn) the entity
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.user_scope_mut(&client_b_key).unwrap().exclude(&entity_e);
        });
    });

    // t2: B despawns entity on scope exit
    // t1: A retains entity — owner never despawns own entity due to non-owner scope changes
    scenario.expect(|ctx| {
        let a_has = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_has = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        // B must have lost it, A must still have it
        (a_has && !b_has).then_some(())
    });

    // Explicit: A must not have received a despawn event in this tick
    // (B's scope exit must not propagate a despawn to the owning client)
    scenario.expect(|ctx| {
        let a_no_despawn = !ctx.client(client_a_key, |c| c.has::<ClientDespawnEntityEvent>());
        a_no_despawn.then_some(())
    });

    // Owner's entity is still present and correctly owned
    scenario.expect(|ctx| {
        let a_has = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let a_owns = ctx.client(client_a_key, |c| {
            c.entity(&entity_e)
                .map(|e| e.owner() == EntityOwner::Client(client_a_key))
                .unwrap_or(false)
        });
        (a_has && a_owns).then_some(())
    });
}
