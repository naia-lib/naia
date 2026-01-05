use naia_client::{ClientConfig, ReplicationConfig as ClientReplicationConfig};
use naia_server::{ReplicationConfig, ServerConfig};
use naia_shared::Protocol;
use naia_test::{protocol, Auth, ClientKey, Position, Scenario};
use test_helpers::test_client_config;

mod test_helpers;
use test_helpers::client_connect;

// ============================================================================
// Domain 4.1: Client-Owned Entities (Unpublished vs Published)
// ============================================================================

/// Client-owned (Unpublished) is visible only to owner
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

/// Client-owned (Published) rejects non-owner mutations
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

/// Client-owned (Published) accepts owner mutations and propagates
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

/// Publish toggle: Published → Unpublished forcibly despawns for non-owners
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

/// Client-owned entities emit NO authority events
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

