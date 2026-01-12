#![allow(unused_imports)]

use std::time::Duration;

use naia_client::{ClientConfig, JitterBufferType, ReplicationConfig as ClientReplicationConfig};
use naia_server::{ReplicationConfig, RoomKey, ServerConfig};
use naia_shared::{AuthorityError, EntityAuthStatus, Protocol, Request, Response, Tick};

use naia_test::{
    protocol, Auth, ClientConnectEvent, ClientDisconnectEvent, ClientEntityAuthDeniedEvent,
    ClientEntityAuthGrantedEvent, ClientEntityAuthResetEvent, ClientKey, ClientRejectEvent,
    ExpectCtx, Position, Scenario, ServerAuthEvent, ServerConnectEvent, ServerDisconnectEvent,
    ToTicks,
};

// Test protocol types (channels and messages)
use naia_test::test_protocol::{
    OrderedChannel, ReliableChannel, RequestResponseChannel, SequencedChannel,
    TestMessage, TestRequest, TestResponse, TickBufferedChannel, UnorderedChannel,
    UnreliableChannel,
};

mod _helpers;
use _helpers::{client_connect, server_and_client_connected, server_and_client_disconnected, test_client_config};


// ============================================================================
// Entity Publication Tests
// ============================================================================
// Tests organized by contract ID to match specs/contracts/9_entity_publication.md
// ============================================================================

/// Client-owned entities emit NO authority events
/// Contract: [entity-publication-01], [entity-publication-03]
///
/// Given client-owned E (Published or Unpublished); when any replication and mutations occur; then clients MUST observe **no** AuthGranted/AuthDenied/AuthLost events for E.
#[test]
fn client_owned_entities_emit_no_authority_events() {
    use naia_test::{
        ClientEntityAuthDeniedEvent, ClientEntityAuthGrantedEvent, ClientEntityAuthResetEvent,
    };

    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        test_client_config(),
        test_protocol.clone(),
    );
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        test_client_config(),
        test_protocol,
    );

    // Client A spawns Published entity
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

    scenario.allow_flexible_next();

    // Put entity in room and include in B's scope
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.enter_room(&room_key);
            }
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity_e);
        });
    });

    // Wait for B to see E
    scenario.expect(|ctx| ctx.client(client_b_key, |c| c.has_entity(&entity_e)).then_some(()));

    // A mutates E
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            if let Some(mut entity_mut) = client_a.entity_mut(&entity_e) {
                entity_mut.insert_component(Position::new(10.0, 20.0));
            }
        });
    });

    // Wait for mutations to propagate
    scenario.expect(|ctx| {
        let a_pos = ctx.client(client_a_key, |c| {
            if let Some(e) = c.entity(&entity_e) {
                e.component::<Position>().map(|p| (*p.x, *p.y))
            } else {
                None
            }
        });
        a_pos
            .filter(|(x, y)| (*x - 10.0).abs() < 0.001 && (*y - 20.0).abs() < 0.001)
            .map(|_| ())
    });

    // Verify no authority events were emitted for A or B
    scenario.mutate(|_ctx| {});
    scenario.expect(|ctx| {
        // Check that no auth events were emitted
        let a_auth_granted = ctx.client(client_a_key, |c| {
            c.read_event::<ClientEntityAuthGrantedEvent>().is_some()
        });
        let a_auth_denied = ctx.client(client_a_key, |c| {
            c.read_event::<ClientEntityAuthDeniedEvent>().is_some()
        });
        let a_auth_reset = ctx.client(client_a_key, |c| {
            c.read_event::<ClientEntityAuthResetEvent>().is_some()
        });
        let b_auth_granted = ctx.client(client_b_key, |c| {
            c.read_event::<ClientEntityAuthGrantedEvent>().is_some()
        });
        let b_auth_denied = ctx.client(client_b_key, |c| {
            c.read_event::<ClientEntityAuthDeniedEvent>().is_some()
        });
        let b_auth_reset = ctx.client(client_b_key, |c| {
            c.read_event::<ClientEntityAuthResetEvent>().is_some()
        });

        // All should be false (no events)
        (!a_auth_granted
            && !a_auth_denied
            && !a_auth_reset
            && !b_auth_granted
            && !b_auth_denied
            && !b_auth_reset)
            .then_some(())
    });
}

/// Client-owned (Published) accepts owner mutations and propagates
/// Contract: [entity-publication-01], [entity-publication-03]
///
/// Given client-owned Published E owned by A and in scope for B; when A mutates E; then server accepts and both A and B observe the updated state.
#[test]
fn client_owned_published_accepts_owner_mutations_and_propagates() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        test_client_config(),
        test_protocol.clone(),
    );
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        test_client_config(),
        test_protocol,
    );

    // Client A spawns Published entity
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

    scenario.allow_flexible_next();

    // Put entity in room and include in B's scope
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.enter_room(&room_key);
            }
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity_e);
        });
    });

    // Wait for B to see E
    scenario.expect(|ctx| ctx.client(client_b_key, |c| c.has_entity(&entity_e)).then_some(()));

    // A mutates E
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            if let Some(mut entity_mut) = client_a.entity_mut(&entity_e) {
                entity_mut.insert_component(Position::new(10.0, 20.0));
            }
        });
    });

    // Verify both A and B observe the updated state
    scenario.expect(|ctx| {
        let a_pos = ctx.client(client_a_key, |c| {
            if let Some(e) = c.entity(&entity_e) {
                e.component::<Position>().map(|p| (*p.x, *p.y))
            } else {
                None
            }
        });
        let b_pos = ctx.client(client_b_key, |c| {
            if let Some(e) = c.entity(&entity_e) {
                e.component::<Position>().map(|p| (*p.x, *p.y))
            } else {
                None
            }
        });

        if let (Some((ax, ay)), Some((bx, by))) = (a_pos, b_pos) {
            let a_correct = (ax - 10.0).abs() < 0.001 && (ay - 20.0).abs() < 0.001;
            let b_correct = (bx - 10.0).abs() < 0.001 && (by - 20.0).abs() < 0.001;
            let same = (ax - bx).abs() < 0.001 && (ay - by).abs() < 0.001;
            (a_correct && b_correct && same).then_some(())
        } else {
            None
        }
    });
}

/// Client-owned (Published) rejects non-owner mutations
/// Contract: [entity-publication-01], [entity-publication-09]
///
/// Given client-owned Published E owned by A and in scope for B; when B attempts to mutate E; then server ignores/rejects B's mutation and authoritative state remains driven by A (and/or server replication), with no panics.
#[test]
fn client_owned_published_rejects_non_owner_mutations() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        test_client_config(),
        test_protocol.clone(),
    );
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        test_client_config(),
        test_protocol,
    );

    // Client A spawns Published entity
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

    scenario.allow_flexible_next();

    // Put entity in room and include in B's scope
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.enter_room(&room_key);
            }
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity_e);
        });
    });

    // Wait for B to see E
    scenario.expect(|ctx| ctx.client(client_b_key, |c| c.has_entity(&entity_e)).then_some(()));

    // B attempts to mutate E (should be ignored/rejected)
    scenario.mutate(|ctx| {
        ctx.client(client_b_key, |client_b| {
            if let Some(mut entity_mut) = client_b.entity_mut(&entity_e) {
                entity_mut.insert_component(Position::new(99.0, 99.0));
            }
        });
    });

    // Verify authoritative state remains (A's original value or server's value)
    scenario.expect(|ctx| {
        let server_pos = ctx.server(|server| {
            if let Some(e) = server.entity(&entity_e) {
                e.component::<Position>().map(|p| (*p.x, *p.y))
            } else {
                None
            }
        });
        // Server should still have original value (not 99, 99)
        if let Some((x, y)) = server_pos {
            // Should be original value (1, 2) or A's value if A updated it
            // Since A hasn't updated, should be (1, 2)
            ((x - 99.0).abs() > 0.001 && (y - 99.0).abs() > 0.001).then_some(())
        } else {
            None
        }
    });
}

/// Client-owned (Unpublished) is visible only to owner
/// Contract: [entity-publication-02], [entity-publication-07]
///
/// Given client A owns client-owned entity E in **Unpublished** state; when E exists; then A can see E, server can see E, and every non-owner client B MUST NOT have E in scope (E absent in B's world).
#[test]
fn client_owned_unpublished_is_visible_only_to_owner() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        test_client_config(),
        test_protocol.clone(),
    );
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        test_client_config(),
        test_protocol,
    );

    // Client A spawns entity (defaults to Private/Unpublished)
    let entity_e = scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            client_a.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
            })
        })
    });

    // Wait for entity to replicate to server
    scenario.expect(|ctx| ctx.server(|server| server.has_entity(&entity_e).then_some(())));

    // Add mutate between expects
    scenario.mutate(|_ctx| {});

    // Verify A can see E, server can see E, but B cannot see E
    scenario.expect(|ctx| {
        let a_sees_e = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let server_sees_e = ctx.server(|server| server.has_entity(&entity_e));
        let b_sees_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        (a_sees_e && server_sees_e && !b_sees_e).then_some(())
    });
}

/// Client-owned (Unpublished) replication is owner→server only
/// Contract: [entity-publication-02], [entity-publication-07]
///
/// Given client-owned Unpublished E owned by A; when A mutates E; then server reflects the mutation; and any non-owner client B never observes E (no visibility, no replication to B).
#[test]
fn client_owned_unpublished_replication_is_owner_to_server_only() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        test_client_config(),
        test_protocol.clone(),
    );
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        test_client_config(),
        test_protocol,
    );

    // Client A spawns Unpublished entity
    let entity_e = scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            client_a.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
            })
        })
    });

    // Wait for entity to replicate to server
    scenario.expect(|ctx| ctx.server(|server| server.has_entity(&entity_e).then_some(())));

    // A mutates E
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            if let Some(mut entity_mut) = client_a.entity_mut(&entity_e) {
                entity_mut.insert_component(Position::new(10.0, 20.0));
            }
        });
    });

    // Verify server reflects the mutation
    scenario.expect(|ctx| {
        let server_pos = ctx.server(|server| {
            if let Some(e) = server.entity(&entity_e) {
                e.component::<Position>().map(|p| (*p.x, *p.y))
            } else {
                None
            }
        });
        if let Some((x, y)) = server_pos {
            ((x - 10.0).abs() < 0.001 && (y - 20.0).abs() < 0.001).then_some(())
        } else {
            None
        }
    });

    // Verify B never observes E
    scenario.mutate(|_ctx| {});
    scenario.expect(|ctx| {
        let b_sees_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        (!b_sees_e).then_some(())
    });
}

/// Client-owned (Published) may be scoped to non-owners
/// Contract: [entity-publication-03], [entity-publication-06]
///
/// Given client-owned Published E owned by A; when server includes E in B's scope; then B observes E (E appears in B's world) with correct replicated state.
#[test]
fn client_owned_published_may_be_scoped_to_non_owners() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        test_client_config(),
        test_protocol.clone(),
    );
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        test_client_config(),
        test_protocol,
    );

    // Client A spawns entity and publishes it
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

    scenario.allow_flexible_next();

    // Put entity in room (needed for scoping)
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.enter_room(&room_key);
            }
        });
    });

    scenario.allow_flexible_next();

    // Server includes E in B's scope
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity_e);
        });
    });

    // Verify B observes E with correct state
    scenario.expect(|ctx| {
        let b_sees_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        if b_sees_e {
            let b_pos = ctx.client(client_b_key, |c| {
                if let Some(e) = c.entity(&entity_e) {
                    e.component::<Position>().map(|p| (*p.x, *p.y))
                } else {
                    None
                }
            });
            if let Some((x, y)) = b_pos {
                ((x - 1.0).abs() < 0.001 && (y - 2.0).abs() < 0.001).then_some(())
            } else {
                None
            }
        } else {
            None
        }
    });
}

/// Only owning client may change publication for client-owned entities
/// Contract: [entity-publication-04]
///
/// Given client-owned entity E owned by A; when A changes publication from Private→Public;
/// then the change takes effect and non-owners can be scoped to E.
/// When A changes publication from Public→Private; then non-owners are removed from scope.
///
/// Note: For client-owned entities, publication changes are driven by the owning client.
/// Server conflicts are resolved per spec, but the primary mechanism is client-initiated.
#[test]
fn only_owner_or_server_may_change_publication() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        test_client_config(),
        test_protocol.clone(),
    );
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        test_client_config(),
        test_protocol,
    );

    // Client A spawns Private (Unpublished) entity
    let entity_e = scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            client_a.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                // Default is Private
            })
        })
    });

    // Wait for entity to replicate to server
    scenario.expect(|ctx| ctx.server(|server| server.has_entity(&entity_e).then_some(())));

    // Verify B does not see E (Private)
    scenario.mutate(|_| {});
    scenario.expect(|ctx| (!ctx.client(client_b_key, |c| c.has_entity(&entity_e))).then_some(()));

    // Owner A publishes E (Private → Public)
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            if let Some(mut entity_mut) = client_a.entity_mut(&entity_e) {
                entity_mut.configure_replication(ClientReplicationConfig::Public);
            }
        });
    });

    // Verify server observes the publication change
    scenario.expect(|ctx| {
        let config = ctx.server(|server| server.entity(&entity_e)?.replication_config());
        (config == Some(ReplicationConfig::Public)).then_some(())
    });

    scenario.allow_flexible_next();

    // Server can now scope E to B
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.enter_room(&room_key);
            }
            server.user_scope_mut(&client_b_key).unwrap().include(&entity_e);
        });
    });

    // Verify B sees E now that it's Published
    scenario.expect(|ctx| ctx.client(client_b_key, |c| c.has_entity(&entity_e)).then_some(()));

    // Owner A unpublishes E (Public → Private)
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            if let Some(mut entity_mut) = client_a.entity_mut(&entity_e) {
                entity_mut.configure_replication(ClientReplicationConfig::Private);
            }
        });
    });

    // Verify B loses E (owner changed to Private)
    scenario.expect(|ctx| {
        let b_sees_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        let config = ctx.server(|server| server.entity(&entity_e)?.replication_config());
        (!b_sees_e && config == Some(ReplicationConfig::Private)).then_some(())
    });
}

/// Publish toggle: Published → Unpublished forcibly despawns for non-owners
/// Contract: [entity-publication-05], [entity-publication-07], [entity-publication-08]
///
/// Given client-owned Published E owned by A and currently in scope for B; when E becomes Unpublished (by server or owner A); then B MUST lose E from its world (OutOfScope), while A and server retain E.
#[test]
fn publish_toggle_published_to_unpublished_forcibly_despawns_for_non_owners() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        test_client_config(),
        test_protocol.clone(),
    );
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        test_client_config(),
        test_protocol,
    );

    // Client A spawns Published entity
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

    scenario.allow_flexible_next();

    // Put entity in room and include in B's scope
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.enter_room(&room_key);
            }
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity_e);
        });
    });

    // Wait for B to see E
    scenario.expect(|ctx| ctx.client(client_b_key, |c| c.has_entity(&entity_e)).then_some(()));

    // Client A unpublishes E (changes to Private)
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            if let Some(mut entity_mut) = client_a.entity_mut(&entity_e) {
                entity_mut.configure_replication(ClientReplicationConfig::Private);
            }
        });
    });

    // Verify B loses E, but A and server retain E
    scenario.expect(|ctx| {
        let a_sees_e = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let server_sees_e = ctx.server(|server| server.has_entity(&entity_e));
        let b_sees_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        (a_sees_e && server_sees_e && !b_sees_e).then_some(())
    });
}

/// Publish toggle: Unpublished → Published enables scoping to non-owners
/// Contract: [entity-publication-06], [entity-publication-09]
///
/// Given client-owned Unpublished E owned by A; when E becomes Published; then server can include E in B's scope and B observes E normally.
#[test]
fn publish_toggle_unpublished_to_published_enables_scoping_to_non_owners() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        test_client_config(),
        test_protocol.clone(),
    );
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        test_client_config(),
        test_protocol,
    );

    // Client A spawns Unpublished entity
    let entity_e = scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            client_a.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
            })
        })
    });

    // Wait for entity to replicate to server
    scenario.expect(|ctx| ctx.server(|server| server.has_entity(&entity_e).then_some(())));

    // Verify B does not see E initially
    scenario.mutate(|_ctx| {});
    scenario.expect(|ctx| (!ctx.client(client_b_key, |c| c.has_entity(&entity_e))).then_some(()));

    // Client A publishes E
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            if let Some(mut entity_mut) = client_a.entity_mut(&entity_e) {
                entity_mut.configure_replication(ClientReplicationConfig::Public);
            }
        });
    });

    scenario.allow_flexible_next();

    // Put entity in room and include in B's scope
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.enter_room(&room_key);
            }
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity_e);
        });
    });

    // Verify B observes E normally
    scenario.expect(|ctx| {
        let b_sees_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        if b_sees_e {
            let b_pos = ctx.client(client_b_key, |c| {
                if let Some(e) = c.entity(&entity_e) {
                    e.component::<Position>().map(|p| (*p.x, *p.y))
                } else {
                    None
                }
            });
            if let Some((x, y)) = b_pos {
                ((x - 1.0).abs() < 0.001 && (y - 2.0).abs() < 0.001).then_some(())
            } else {
                None
            }
        } else {
            None
        }
    });
}

/// Delegation migration ends client-owned publication semantics
/// Contract: [entity-publication-10]
///
/// Given client-owned Published entity E owned by A; when E migrates to delegated (via client);
/// then publication semantics no longer apply and E is governed by delegated authority rules.
///
/// Note: Delegation migration for client-owned entities must be initiated by the owning client,
/// not the server. The server cannot downgrade client ownership to delegated.
#[test]
fn delegation_migration_ends_client_owned_publication_semantics() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        test_client_config(),
        test_protocol.clone(),
    );
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        test_client_config(),
        test_protocol,
    );

    // Client A spawns Published entity
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

    scenario.allow_flexible_next();

    // Put entity in room and scope to B
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.enter_room(&room_key);
            }
            server.user_scope_mut(&client_b_key).unwrap().include(&entity_e);
        });
    });

    // Wait for B to see E
    scenario.expect(|ctx| ctx.client(client_b_key, |c| c.has_entity(&entity_e)).then_some(()));

    // Client A migrates E to delegated (client-owned Published → delegated)
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            if let Some(mut entity_mut) = client_a.entity_mut(&entity_e) {
                entity_mut.configure_replication(ClientReplicationConfig::Delegated);
            }
        });
    });

    // Verify: E is now Delegated and both clients observe authority semantics
    scenario.expect(|ctx| {
        let config = ctx.server(|server| server.entity(&entity_e)?.replication_config());
        let config_ok = config == Some(ReplicationConfig::Delegated);

        // After delegation migration, clients should have authority status (not None)
        let a_has_auth = ctx.client(client_a_key, |c| {
            c.entity(&entity_e).and_then(|e| e.authority()).is_some()
        });
        let b_has_auth = ctx.client(client_b_key, |c| {
            c.entity(&entity_e).and_then(|e| e.authority()).is_some()
        });

        (config_ok && a_has_auth && b_has_auth).then_some(())
    });
}

/// Non-owner observing Private entity must immediately despawn it
/// Contract: [entity-publication-11]
///
/// This contract defines defensive behavior: if a non-owner client ever sees a Private entity
/// (which should never happen in correct usage), it MUST despawn it.
/// We test this by verifying that the existing Publication→Unpublished flow correctly despawns
/// for non-owners, which exercises the same code path.
#[test]
fn non_owner_seeing_private_must_despawn() {
    // This test verifies the behavior that implements entity-publication-11:
    // When a non-owner observes that an entity becomes Private, it must despawn.
    // The publish_toggle_published_to_unpublished_forcibly_despawns_for_non_owners test
    // already exercises the primary path, but this test specifically annotates and verifies
    // the contract requirement that Private entities are immediately treated as OutOfScope.

    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        test_client_config(),
        test_protocol.clone(),
    );
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        test_client_config(),
        test_protocol,
    );

    // Client A spawns Published entity
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

    scenario.allow_flexible_next();

    // Scope E to B
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.enter_room(&room_key);
            }
            server.user_scope_mut(&client_b_key).unwrap().include(&entity_e);
        });
    });

    // Wait for B to see E
    scenario.expect(|ctx| ctx.client(client_b_key, |c| c.has_entity(&entity_e)).then_some(()));

    // Verify B currently sees the entity as Public (not Private)
    scenario.mutate(|_| {});
    scenario.expect(|ctx| {
        let b_config = ctx.client(client_b_key, |c| {
            c.entity(&entity_e).and_then(|e| e.replication_config())
        });
        // B should see it as Public (non-private)
        (b_config == Some(ClientReplicationConfig::Public)).then_some(())
    });

    // Change to Private - B should immediately despawn (entity-publication-11 behavior)
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            if let Some(mut entity_mut) = client_a.entity_mut(&entity_e) {
                entity_mut.configure_replication(ClientReplicationConfig::Private);
            }
        });
    });

    // Verify: B no longer has E (despawned due to Private = OutOfScope for non-owner)
    // A still has E (owner always in scope)
    scenario.expect(|ctx| {
        let a_has_e = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_has_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        (a_has_e && !b_has_e).then_some(())
    });
}
