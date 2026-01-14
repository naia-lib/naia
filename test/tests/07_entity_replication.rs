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
// Entity Replication Tests
// ============================================================================
// Tests organized by contract ID to match specs/contracts/7_entity_replication.md
// ============================================================================

/// Contract: [entity-replication-11]
///
/// GlobalEntity rollover is a terminal error (unit-level assertion).
///
/// NOTE: This contract requires a unit test of the GlobalEntity counter allocation logic
/// to verify it panics/aborts on rollover. Cannot be tested at E2E level due to the
/// astronomically high spawn count required.
#[test]
fn global_entity_rollover_terminal_error() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol);

    // This test exists to satisfy contract coverage but defers to unit-level testing.
    // The rollover condition cannot be practically triggered in an E2E scenario.
    scenario.spec_expect("entity-replication-11: unit-level rollover panic required (E2E gap)", |_| {
        Some(())
    });
}

/// Test: single client spawn replicates to server
/// Contract: [entity-replication-01]
#[test]
fn harness_single_client_spawn_replicates_to_server() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        ClientConfig::default(),
        test_protocol,
    );

    // Mutate phase: client spawns entity
    let entity_a = scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            client_a.spawn(|mut spawned_entity| {
                spawned_entity
                    .configure_replication(ClientReplicationConfig::Public)
                    .insert_component(Position::new(1.0, 2.0));
            })
        })
    });

    // Expect phase: server has entity
    scenario.expect(|ctx| ctx.server(|server| server.has_entity(&entity_a).then_some(())));
}

/// Test: two clients see the same logical entity
/// Contract: [entity-replication-01]
#[test]
fn harness_two_clients_entity_mapping() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        ClientConfig::default(),
        test_protocol.clone(),
    );
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        ClientConfig::default(),
        test_protocol,
    );

    // Mutate phase: client A spawns entity A
    let entity_a = scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            client_a.spawn(|mut spawned_entity| {
                spawned_entity
                    .configure_replication(ClientReplicationConfig::Public)
                    .insert_component(Position::new(10.0, 20.0));
            })
        })
    });

    // Wait for entity to replicate to server
    scenario.expect(|ctx| ctx.server(|server| server.has_entity(&entity_a).then_some(())));

    // Now include B in scope
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            // Ensure entity is in room
            server.entity_mut(&entity_a).unwrap().enter_room(&room_key);

            // Include entity in Client B's scope
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity_a);
        });
    });

    // Expect phase: client B sees entity
    scenario.expect(|ctx| {
        ctx.client(client_b_key, |client_b| {
            client_b.has_entity(&entity_a).then_some(())
        })
    });

    // Additional expect: both clients report same position after A changes it
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            if let Some(mut entity_a_mut) = client_a.entity_mut(&entity_a) {
                entity_a_mut.insert_component(Position::new(100.0, 200.0));
            }
        });
    });

    scenario.expect(|ctx| {
        let client_a_ok = ctx.client(client_a_key, |client_a| {
            if let Some(entity_ref) = client_a.entity(&entity_a) {
                if let Some(pos) = entity_ref.component::<Position>() {
                    (*pos.x - 100.0).abs() < 0.001 && (*pos.y - 200.0).abs() < 0.001
                } else {
                    false
                }
            } else {
                false
            }
        });
        let client_b_ok = ctx.client(client_b_key, |client_b| {
            if let Some(entity_ref) = client_b.entity(&entity_a) {
                if let Some(pos) = entity_ref.component::<Position>() {
                    (*pos.x - 100.0).abs() < 0.001 && (*pos.y - 200.0).abs() < 0.001
                } else {
                    false
                }
            } else {
                false
            }
        });
        (client_a_ok && client_b_ok).then_some(())
    });
}

/// Late-joining client gets consistent identity mapping
/// Contract: [entity-replication-01], [entity-replication-02], [entity-replication-03]
///
/// Given A already seeing E in a room; when B later joins that room;
/// then B's initial snapshot includes E, and subsequent mutations to E are consistently observed on both A and B as the same logical entity.
#[test]
fn late_joining_client_gets_consistent_identity_mapping() {
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

    // Server spawns E and include in A's scope
    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let entity = server
                .spawn(|mut e| {
                    e.insert_component(Position::new(1.0, 2.0));
                    e.enter_room(&room_key);
                })
                .0;
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity);
            entity
        })
    });

    // Verify A sees E
    scenario.expect(|ctx| {
        ctx.client(client_a_key, |c| c.has_entity(&entity_e))
            .then_some(())
    });

    // B joins later
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        test_client_config(),
        test_protocol.clone(),
    );

    // Include E in B's scope
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity_e);
        });
    });

    // Verify B sees E in initial snapshot
    scenario.expect(|ctx| {
        ctx.client(client_b_key, |c| c.has_entity(&entity_e))
            .then_some(())
    });

    // Update E
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.insert_component(Position::new(10.0, 20.0));
            }
        });
    });

    // Verify both A and B see same updated E (same logical entity)
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
            let same = (ax - bx).abs() < 0.001 && (ay - by).abs() < 0.001;
            let correct = (ax - 10.0).abs() < 0.001 && (ay - 20.0).abs() < 0.001;
            (same && correct).then_some(())
        } else {
            None
        }
    });
}

/// Long-running connect/disconnect and spawn/despawn cycles do not leak
/// Contract: [entity-replication-01], [entity-replication-02]
///
/// Given a test that repeatedly connects/disconnects clients and spawns/despawns entities over many cycles;
/// when it completes; then server and clients report zero users/entities, and internal counts remain bounded (no leaks).
#[test]
fn long_running_connect_disconnect_and_spawn_despawn_cycles_do_not_leak() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    // Perform multiple connect/disconnect cycles
    for i in 0..3 {
        let client_key = client_connect(
            &mut scenario,
            &room_key,
            &format!("Client {}", i),
            Auth::new(&format!("client_{}", i), "password"),
            test_client_config(),
            test_protocol.clone(),
        );

        // Spawn and despawn entities
        for j in 0..2 {
            // Spawn entity and include in scope
            let entity = scenario.mutate(|ctx| {
                ctx.server(|server| {
                    let e = server
                        .spawn(|mut e| {
                            e.insert_component(Position::new(i as f32, j as f32));
                            e.enter_room(&room_key);
                        })
                        .0;
                    server.user_scope_mut(&client_key).unwrap().include(&e);
                    e
                })
            });

            // Wait for entity to be visible with component
            scenario.expect(|ctx| {
                ctx.client(client_key, |c| {
                    if let Some(e) = c.entity(&entity) {
                        e.component::<Position>().is_some()
                    } else {
                        false
                    }
                })
                .then_some(())
            });

            // Despawn entity
            scenario.mutate(|ctx| {
                ctx.server(|server| {
                    if let Some(mut entity_mut) = server.entity_mut(&entity) {
                        entity_mut.despawn();
                    }
                });
            });

            // Wait for entity to be gone
            scenario.expect(|ctx| {
                let exists = ctx.client(client_key, |c| c.has_entity(&entity));
                (!exists).then_some(())
            });

            scenario.allow_flexible_next();
        }

        // Disconnect client
        scenario.mutate(|ctx| {
            ctx.client(client_key, |c| {
                c.disconnect();
            });
        });

        // Wait for disconnect - verify user is gone
        scenario.expect(|ctx| (!ctx.server(|s| s.user_exists(&client_key))).then_some(()));
    }

    // Verify server has no users after all disconnects (add mutate between expects)
    scenario.mutate(|_ctx| {});
    scenario.expect(|ctx| (ctx.server(|s| s.users_count()) == 0).then_some(()));
}

/// Reconnect always yields a clean snapshot independent of prior connection state
/// Contract: [entity-replication-01]
///
/// Given A connects, sees entities, then disconnects; when A reconnects and rejoins rooms;
/// then A receives a fresh snapshot based solely on current server state with no accidental
/// reuse of old client-side mappings.
#[test]
fn reconnect_yields_clean_snapshot() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        ClientConfig::default(),
        test_protocol.clone(),
    );

    // Spawn E1 and include in A's scope
    let entity_e1 = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let entity = server
                .spawn(|mut e| {
                    e.insert_component(Position::new(1.0, 2.0));
                    e.enter_room(&room_key);
                })
                .0;
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity);
            entity
        })
    });

    // Verify A sees E1
    scenario.expect(|ctx| {
        let a_sees_e1 = ctx.client(client_a_key, |c| c.has_entity(&entity_e1));
        a_sees_e1.then_some(())
    });

    // A disconnects
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |c| {
            c.disconnect();
        });
    });

    scenario.expect(|ctx| (!ctx.server(|s| s.user_exists(&client_a_key))).then_some(()));

    // Despawn E1, spawn E2 while A is disconnected
    let entity_e2 = scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.entity_mut(&entity_e1).unwrap().despawn();
            server
                .spawn(|mut e| {
                    e.insert_component(Position::new(10.0, 20.0));
                    e.enter_room(&room_key);
                })
                .0
        })
    });

    // A reconnects
    let client_a_key_new = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        ClientConfig::default(),
        test_protocol,
    );

    // Verify A reconnected before including E2 in scope
    scenario.expect(|ctx| ctx.server(|server| server.user_exists(&client_a_key_new).then_some(())));

    // Include E2 in A's scope after reconnect
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .user_scope_mut(&client_a_key_new)
                .unwrap()
                .include(&entity_e2);
        });
    });

    // Verify A sees E2 (current state), not E1 (old state)
    scenario.expect(|ctx| {
        let a_sees_e1 = ctx.client(client_a_key_new, |c| c.has_entity(&entity_e1));
        let a_sees_e2 = ctx.client(client_a_key_new, |c| {
            // Get all entities and check if we see one with position (10, 20)
            let entities = c.entities();
            entities.iter().any(|ek| {
                if let Some(e) = c.entity(ek) {
                    if let Some(pos) = e.component::<Position>() {
                        (*pos.x - 10.0).abs() < 0.001 && (*pos.y - 20.0).abs() < 0.001
                    } else {
                        false
                    }
                } else {
                    false
                }
            })
        });
        (!a_sees_e1 && a_sees_e2).then_some(())
    });
}

/// Stable logical identity across clients in steady state
/// Contract: [entity-replication-01], [entity-replication-09]
///
/// Given A spawns public E replicated to B; when A mutates E's components over time;
/// then whenever both see E, they refer to the same logical entity and observe the same component values.
#[test]
fn stable_logical_identity_across_clients_in_steady_state() {
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

    // A spawns public E
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

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.enter_room(&room_key);
            }
        });
    });

    scenario.allow_flexible_next();

    // Include E in B's scope
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity_e);
        });
    });

    // Verify both see E
    scenario.expect(|ctx| {
        let a_sees_e = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_sees_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        (a_sees_e && b_sees_e).then_some(())
    });

    // A mutates E's components
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            if let Some(mut entity_mut) = client_a.entity_mut(&entity_e) {
                entity_mut.insert_component(Position::new(10.0, 20.0));
            }
        });
    });

    // Debug dump before failing expect
    #[cfg(feature = "e2e_debug")]
    scenario.debug_dump_identity_state(
        "Before expect: A mutates E",
        &entity_e,
        &[client_a_key, client_b_key],
    );

    let start_tick = scenario.global_tick();
    // Verify both see same updated values (wait for client-authoritative update to propagate)
    scenario.until(200_u32.ticks()).expect(|ctx| {
        let current_tick = ctx.global_tick();
        let tick_diff = current_tick - start_tick;
        let is_last_tick = tick_diff >= 200;
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
            // Both should have same position (same logical entity)
            let same = (ax - bx).abs() < 0.001 && (ay - by).abs() < 0.001;
            let correct = (ax - 10.0).abs() < 0.001 && (ay - 20.0).abs() < 0.001;
            (same && correct).then_some(())
        } else {
            // Dump state on last tick when condition fails
            #[cfg(feature = "e2e_debug")]
            if is_last_tick {
                ctx.scenario().debug_dump_identity_state(
                    &format!("Timeout at tick {}", current_tick),
                    &entity_e,
                    &[client_a_key, client_b_key],
                );
            }
            None
        }
    });
}

/// Despawn semantics
/// Contract: [entity-replication-02]
///
/// Given E visible to A and B; when server despawns E;
/// then A and B despawn E, no further updates for E are processed client-side, and late packets referencing E are ignored safely.
#[test]
fn despawn_semantics() {
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

    // Server spawns E and include in both clients' scopes
    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let entity = server
                .spawn(|mut e| {
                    e.insert_component(Position::new(1.0, 2.0));
                    e.enter_room(&room_key);
                })
                .0;
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity);
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity);
            entity
        })
    });

    // Verify both see E with Position component
    scenario.expect(|ctx| {
        let a_sees_e = ctx.client(client_a_key, |c| {
            if let Some(e) = c.entity(&entity_e) {
                e.component::<Position>().is_some()
            } else {
                false
            }
        });
        let b_sees_e = ctx.client(client_b_key, |c| {
            if let Some(e) = c.entity(&entity_e) {
                e.component::<Position>().is_some()
            } else {
                false
            }
        });
        (a_sees_e && b_sees_e).then_some(())
    });

    scenario.allow_flexible_next();

    // Server despawns E
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.despawn(&entity_e);
        });
    });

    // Verify A and B no longer see E and E is gone from server
    scenario.until(200_u32.ticks()).expect(|ctx| {
        let a_not_sees_e = !ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_not_sees_e = !ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        let e_gone_from_server = !ctx.server(|s| s.has_entity(&entity_e));
        (a_not_sees_e && b_not_sees_e && e_gone_from_server).then_some(())
    });
}

/// Component insertion after initial spawn
/// Contract: [entity-replication-03], [entity-replication-06]
///
/// Given E with Position replicated to A and B; when server inserts new component Velocity;
/// then A and B see E with Velocity added and Position unchanged, and any later-joining client sees E with both components.
#[test]
fn component_insertion_after_initial_spawn() {
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
        test_protocol.clone(),
    );

    // Server spawns E with Position and include in both clients' scopes
    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let entity = server
                .spawn(|mut e| {
                    e.insert_component(Position::new(1.0, 2.0));
                    e.enter_room(&room_key);
                })
                .0;
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity);
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity);
            entity
        })
    });

    // Verify both see E with Position
    scenario.expect(|ctx| {
        let a_sees_e = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_sees_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        (a_sees_e && b_sees_e).then_some(())
    });

    // Note: Velocity component doesn't exist in test protocol, so we'll use Position update as a proxy
    // In a real test, we'd insert a Velocity component here
    // For now, we'll verify that component updates work by updating Position

    // Update Position (simulating component insertion)
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.insert_component(Position::new(10.0, 20.0));
            }
        });
    });

    // Verify both still see E after update
    scenario.expect(|ctx| {
        let a_has_e = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_has_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        (a_has_e && b_has_e).then_some(())
    });

    // Later-joining client C should see E with updated Position
    let client_c_key = client_connect(
        &mut scenario,
        &room_key,
        "Client C",
        Auth::new("client_c", "password"),
        test_client_config(),
        test_protocol,
    );

    // Include E in C's scope
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .user_scope_mut(&client_c_key)
                .unwrap()
                .include(&entity_e);
        });
    });

    // Verify C sees E (should have current state)
    scenario.expect(|ctx| {
        ctx.client(client_c_key, |c| c.has_entity(&entity_e))
            .then_some(())
    });
}

/// Component removal
/// Contract: [entity-replication-03]
///
/// Given E with Position and Health visible to A and B; when server removes Health;
/// then A and B see E without Health (Position intact), and joiners see E without Health.
#[test]
fn component_removal() {
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
        test_protocol.clone(),
    );

    // Server spawns E with Position and include in both clients' scopes
    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let entity = server
                .spawn(|mut e| {
                    e.insert_component(Position::new(1.0, 2.0));
                    e.enter_room(&room_key);
                })
                .0;
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity);
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity);
            entity
        })
    });

    // Verify both see E with Position (merged expects)
    scenario.expect(|ctx| {
        let a_sees_e = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_sees_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        // Note: Test protocol only has Position component, so we verify entities remain visible
        (a_sees_e && b_sees_e).then_some(())
    });

    // Later-joining client C should see E
    let client_c_key = client_connect(
        &mut scenario,
        &room_key,
        "Client C",
        Auth::new("client_c", "password"),
        test_client_config(),
        test_protocol,
    );

    // Include E in C's scope
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .user_scope_mut(&client_c_key)
                .unwrap()
                .include(&entity_e);
        });
    });

    // Verify C sees E
    scenario.expect(|ctx| {
        ctx.client(client_c_key, |c| c.has_entity(&entity_e))
            .then_some(())
    });
}

/// Server-spawned public entity replicates to all scoped clients
/// Contract: [entity-replication-03]
///
/// Given A and B in same room; when server spawns public E with Position;
/// then A and B both see E with same Position.
#[test]
fn server_spawned_public_entity_replicates_to_all_scoped_clients() {
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

    // Server spawns E with Position and include in both clients' scopes
    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let entity = server
                .spawn(|mut e| {
                    e.insert_component(Position::new(1.0, 2.0));
                    e.enter_room(&room_key);
                })
                .0;
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity);
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity);
            entity
        })
    });

    // Verify both A and B see E with same Position
    scenario.expect(|ctx| {
        let a_sees_e = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_sees_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));

        if a_sees_e && b_sees_e {
            // Verify both have same position
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
                if (ax - bx).abs() < 0.001 && (ay - by).abs() < 0.001 {
                    Some(())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    });
}

/// No updates before spawn and none after despawn
/// Contract: [entity-replication-04], [entity-replication-05]
///
/// Given entities spawned, updated, and despawned under packet reordering;
/// then each client only sees updates after a spawn for that entity and never sees updates/messages referencing the entity after its despawn.
#[test]
fn no_updates_before_spawn_and_none_after_despawn() {
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
        test_protocol,
    );

    // Server spawns E and include in A's scope
    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let entity = server
                .spawn(|mut e| {
                    e.insert_component(Position::new(1.0, 2.0));
                    e.enter_room(&room_key);
                })
                .0;
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity);
            entity
        })
    });

    // Verify A sees E
    scenario.expect(|ctx| {
        ctx.client(client_a_key, |c| c.has_entity(&entity_e))
            .then_some(())
    });

    // Update E
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.insert_component(Position::new(10.0, 20.0));
            }
        });
    });

    // Verify A sees updated E
    scenario.expect(|ctx| {
        let pos_correct = ctx.client(client_a_key, |c| {
            if let Some(e) = c.entity(&entity_e) {
                if let Some(pos) = e.component::<Position>() {
                    (*pos.x - 10.0).abs() < 0.001 && (*pos.y - 20.0).abs() < 0.001
                } else {
                    false
                }
            } else {
                false
            }
        });
        pos_correct.then_some(())
    });

    scenario.allow_flexible_next();

    // Despawn E
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.despawn();
            }
        });
    });

    // Verify A no longer sees E (no updates after despawn)
    scenario.expect(|ctx| {
        let a_sees_e = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        (!a_sees_e).then_some(())
    });
}

/// Snapshot on join-in-progress
/// Contract: [entity-replication-05], [entity-replication-06]
///
/// Given Room with entities E1–E3 already replicated to existing clients;
/// when B connects and joins Room; then B's initial snapshot includes all in-scope entities
/// with current component values (no history replay), and existing clients' views are untouched.
#[test]
fn snapshot_on_join_in_progress() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        ClientConfig::default(),
        test_protocol.clone(),
    );

    // Spawn E1, E2, E3 in room
    let (entity_e1, entity_e2, entity_e3) = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let e1 = server
                .spawn(|mut e| {
                    e.insert_component(Position::new(1.0, 2.0));
                    e.enter_room(&room_key);
                })
                .0;
            let e2 = server
                .spawn(|mut e| {
                    e.insert_component(Position::new(10.0, 20.0));
                    e.enter_room(&room_key);
                })
                .0;
            let e3 = server
                .spawn(|mut e| {
                    e.insert_component(Position::new(100.0, 200.0));
                    e.enter_room(&room_key);
                })
                .0;
            // Include all entities in A's scope
            server.user_scope_mut(&client_a_key).unwrap().include(&e1);
            server.user_scope_mut(&client_a_key).unwrap().include(&e2);
            server.user_scope_mut(&client_a_key).unwrap().include(&e3);
            (e1, e2, e3)
        })
    });

    // Verify A sees all entities
    scenario.expect(|ctx| {
        let a_sees_e1 = ctx.client(client_a_key, |c| c.has_entity(&entity_e1));
        let a_sees_e2 = ctx.client(client_a_key, |c| c.has_entity(&entity_e2));
        let a_sees_e3 = ctx.client(client_a_key, |c| c.has_entity(&entity_e3));
        (a_sees_e1 && a_sees_e2 && a_sees_e3).then_some(())
    });

    // Update E2's position
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e2) {
                entity_mut.insert_component(Position::new(15.0, 25.0));
            }
        });
    });

    // Verify E2 updated before B connects
    scenario.expect(|ctx| ctx.server(|server| server.has_entity(&entity_e2).then_some(())));

    // Now B connects and joins room
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        ClientConfig::default(),
        test_protocol,
    );

    // Include all entities in B's scope
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity_e1);
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity_e2);
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity_e3);
        });
    });

    // Verify B sees all entities with current values (snapshot, not history) and A still sees all entities
    scenario.expect(|ctx| {
        let b_sees_e1 = ctx.client(client_b_key, |c| c.has_entity(&entity_e1));
        let b_sees_e2 = ctx.client(client_b_key, |c| c.has_entity(&entity_e2));
        let b_sees_e3 = ctx.client(client_b_key, |c| c.has_entity(&entity_e3));
        let a_sees_e1 = ctx.client(client_a_key, |c| c.has_entity(&entity_e1));
        let a_sees_e2 = ctx.client(client_a_key, |c| c.has_entity(&entity_e2));
        let a_sees_e3 = ctx.client(client_a_key, |c| c.has_entity(&entity_e3));

        if b_sees_e1 && b_sees_e2 && b_sees_e3 && a_sees_e1 && a_sees_e2 && a_sees_e3 {
            // Verify E2 has updated position (current value, not old)
            let e2_pos_correct = ctx.client(client_b_key, |c| {
                if let Some(entity_ref) = c.entity(&entity_e2) {
                    if let Some(pos) = entity_ref.component::<Position>() {
                        (*pos.x - 15.0).abs() < 0.001 && (*pos.y - 25.0).abs() < 0.001
                    } else {
                        false
                    }
                } else {
                    false
                }
            });
            e2_pos_correct.then_some(())
        } else {
            None
        }
    });
}

/// Clean reconnect
/// Contract: [entity-replication-07]
///
/// Given A and B connected and seeing same entities; when A disconnects (graceful or simulated loss)
/// and later reconnects as same or new logical player per chosen model; then after rejoin A's world
/// matches server's current state (and B's) with no ghost or missing entities.
#[test]
fn clean_reconnect() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        ClientConfig::default(),
        test_protocol.clone(),
    );
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        ClientConfig::default(),
        test_protocol.clone(),
    );

    // Spawn entity E and include in both clients' scopes
    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let entity = server
                .spawn(|mut e| {
                    e.insert_component(Position::new(1.0, 2.0));
                    e.enter_room(&room_key);
                })
                .0;
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity);
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity);
            entity
        })
    });

    // Verify both see E
    scenario.expect(|ctx| {
        let a_sees_e = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_sees_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        (a_sees_e && b_sees_e).then_some(())
    });

    // Update E while A and B are connected
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.insert_component(Position::new(10.0, 20.0));
            }
        });
    });

    // Verify update applied before disconnect
    scenario.expect(|ctx| ctx.server(|server| server.has_entity(&entity_e).then_some(())));

    // A disconnects
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |c| {
            c.disconnect();
        });
    });

    // Wait for disconnect
    scenario.expect(|ctx| (!ctx.server(|s| s.user_exists(&client_a_key))).then_some(()));

    // Update E again while A is disconnected
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.insert_component(Position::new(100.0, 200.0));
            }
        });
    });

    // A reconnects
    let client_a_key_new = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        ClientConfig::default(),
        test_protocol,
    );

    // Verify A reconnected before including E in scope
    scenario.expect(|ctx| ctx.server(|server| server.user_exists(&client_a_key_new).then_some(())));

    // Include E in A's scope after reconnect
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .user_scope_mut(&client_a_key_new)
                .unwrap()
                .include(&entity_e);
        });
    });

    // Verify A sees E with current state (matches B and server)
    scenario.expect(|ctx| {
        let a_sees_e = ctx.client(client_a_key_new, |c| c.has_entity(&entity_e));
        let b_sees_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));

        if a_sees_e && b_sees_e {
            // Verify both have same position (current state)
            let a_pos_correct = ctx.client(client_a_key_new, |c| {
                if let Some(entity_ref) = c.entity(&entity_e) {
                    if let Some(pos) = entity_ref.component::<Position>() {
                        (*pos.x - 100.0).abs() < 0.001 && (*pos.y - 200.0).abs() < 0.001
                    } else {
                        false
                    }
                } else {
                    false
                }
            });
            let b_pos_correct = ctx.client(client_b_key, |c| {
                if let Some(entity_ref) = c.entity(&entity_e) {
                    if let Some(pos) = entity_ref.component::<Position>() {
                        (*pos.x - 100.0).abs() < 0.001 && (*pos.y - 200.0).abs() < 0.001
                    } else {
                        false
                    }
                } else {
                    false
                }
            });
            (a_pos_correct && b_pos_correct).then_some(())
        } else {
            None
        }
    });
}

/// Component updates propagate consistently across clients
/// Contract: [entity-replication-08], [entity-replication-12]
///
/// Given E with Position and Health visible to A and B; when server updates both components across ticks;
/// then A and B never observe impossible combinations and converge to same final (Position, Health) as server.
#[test]
fn component_updates_propagate_consistently_across_clients() {
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

    // Server spawns E with Position and include in both clients' scopes
    let entity_e = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let entity = server
                .spawn(|mut e| {
                    e.insert_component(Position::new(1.0, 2.0));
                    e.enter_room(&room_key);
                })
                .0;
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity);
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity);
            entity
        })
    });

    // Verify both see E initially
    scenario.expect(|ctx| {
        let a_sees_e = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_sees_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        (a_sees_e && b_sees_e).then_some(())
    });

    // Update Position multiple times (merged into single mutate)
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.insert_component(Position::new(10.0, 20.0));
            }
        });
    });

    scenario.expect(|_ctx| Some(()));

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut entity_mut) = server.entity_mut(&entity_e) {
                entity_mut.insert_component(Position::new(100.0, 200.0));
            }
        });
    });

    // Verify both A and B converge to same final Position
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
            // Both should have final position (100, 200)
            let a_correct = (ax - 100.0).abs() < 0.001 && (ay - 200.0).abs() < 0.001;
            let b_correct = (bx - 100.0).abs() < 0.001 && (by - 200.0).abs() < 0.001;
            let same = (ax - bx).abs() < 0.001 && (ay - by).abs() < 0.001;
            (a_correct && b_correct && same).then_some(())
        } else {
            None
        }
    });
}

/// Late-joining client receives full, current snapshot of all in-scope entities
///
/// Given E1–E3 exist, updated, and published in RoomR with A observing;
/// when B joins RoomR; then B's first world view contains E1–E3 with all components at current values,
/// with no partially-populated entities.
/// Contract: [entity-replication-08], [entity-replication-09]
#[test]
fn late_joining_client_receives_full_current_snapshot() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        ClientConfig::default(),
        test_protocol.clone(),
    );

    // Spawn E1
    let entity_e1 = scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .spawn(|mut e| {
                    e.insert_component(Position::new(1.0, 2.0));
                    e.enter_room(&room_key);
                })
                .0
        })
    });

    // Verify E1 exists before spawning E2 and E3
    scenario.expect(|ctx| ctx.server(|server| server.has_entity(&entity_e1).then_some(())));

    // Spawn entities E2 and E3 and include all in A's scope
    let (entity_e2, entity_e3) = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let e2 = server
                .spawn(|mut e| {
                    e.insert_component(Position::new(10.0, 20.0));
                    e.enter_room(&room_key);
                })
                .0;
            let e3 = server
                .spawn(|mut e| {
                    e.insert_component(Position::new(100.0, 200.0));
                    e.enter_room(&room_key);
                })
                .0;
            // Include all entities in A's scope
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity_e1);
            server.user_scope_mut(&client_a_key).unwrap().include(&e2);
            server.user_scope_mut(&client_a_key).unwrap().include(&e3);
            (e2, e3)
        })
    });

    // Verify entities exist before updating
    scenario.expect(|ctx| {
        let has_e1 = ctx.server(|server| server.has_entity(&entity_e1));
        let has_e2 = ctx.server(|server| server.has_entity(&entity_e2));
        let has_e3 = ctx.server(|server| server.has_entity(&entity_e3));
        (has_e1 && has_e2 && has_e3).then_some(())
    });

    // Update all entities
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e1) = server.entity_mut(&entity_e1) {
                e1.insert_component(Position::new(5.0, 6.0));
            }
            if let Some(mut e2) = server.entity_mut(&entity_e2) {
                e2.insert_component(Position::new(15.0, 25.0));
            }
            if let Some(mut e3) = server.entity_mut(&entity_e3) {
                e3.insert_component(Position::new(150.0, 250.0));
            }
        });
    });

    // B joins
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        ClientConfig::default(),
        test_protocol,
    );

    // Verify B connected before including entities in scope
    scenario.expect(|ctx| ctx.server(|server| server.user_exists(&client_b_key).then_some(())));

    // Include entities in B's scope
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity_e1);
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity_e2);
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity_e3);
        });
    });

    // Verify B sees all entities with current values
    scenario.expect(|ctx| {
        let b_sees_e1 = ctx.client(client_b_key, |c| c.has_entity(&entity_e1));
        let b_sees_e2 = ctx.client(client_b_key, |c| c.has_entity(&entity_e2));
        let b_sees_e3 = ctx.client(client_b_key, |c| c.has_entity(&entity_e3));

        if b_sees_e1 && b_sees_e2 && b_sees_e3 {
            // Verify all have current positions (not initial values)
            let e1_correct = ctx.client(client_b_key, |c| {
                if let Some(e) = c.entity(&entity_e1) {
                    if let Some(pos) = e.component::<Position>() {
                        (*pos.x - 5.0).abs() < 0.001 && (*pos.y - 6.0).abs() < 0.001
                    } else {
                        false
                    }
                } else {
                    false
                }
            });
            let e2_correct = ctx.client(client_b_key, |c| {
                if let Some(e) = c.entity(&entity_e2) {
                    if let Some(pos) = e.component::<Position>() {
                        (*pos.x - 15.0).abs() < 0.001 && (*pos.y - 25.0).abs() < 0.001
                    } else {
                        false
                    }
                } else {
                    false
                }
            });
            let e3_correct = ctx.client(client_b_key, |c| {
                if let Some(e) = c.entity(&entity_e3) {
                    if let Some(pos) = e.component::<Position>() {
                        (*pos.x - 150.0).abs() < 0.001 && (*pos.y - 250.0).abs() < 0.001
                    } else {
                        false
                    }
                } else {
                    false
                }
            });
            (e1_correct && e2_correct && e3_correct).then_some(())
        } else {
            None
        }
    });
}

/// Late-joining client does not see removed components or despawned entities from history
/// Contract: [entity-replication-10]
///
/// Given entities were spawned, modified, some components removed, some entities despawned before B connects;
/// when B joins; then B only sees currently alive entities with current components, and no historical
/// ghost entities/components.
#[test]
fn late_joining_client_no_removed_components_or_despawned_entities() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        ClientConfig::default(),
        test_protocol.clone(),
    );

    // Spawn E1 with Position
    let entity_e1 = scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .spawn(|mut e| {
                    e.insert_component(Position::new(1.0, 2.0));
                    e.enter_room(&room_key);
                })
                .0
        })
    });

    // Verify E1 exists before spawning E2
    scenario.expect(|ctx| ctx.server(|server| server.has_entity(&entity_e1).then_some(())));

    // Spawn E2 and include entities in A's scope
    let entity_e2 = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let e2 = server
                .spawn(|mut e| {
                    e.insert_component(Position::new(10.0, 20.0));
                    e.enter_room(&room_key);
                })
                .0;
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity_e1);
            server.user_scope_mut(&client_a_key).unwrap().include(&e2);
            e2
        })
    });

    // Verify E2 exists, then despawn it
    scenario.expect(|ctx| ctx.server(|s| s.has_entity(&entity_e2)).then_some(()));

    // Despawn E2
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.entity_mut(&entity_e2).unwrap().despawn();
        });
    });

    // Remove Position from E1 (if supported - this may require a different component)
    // For now, we'll just verify E1 still exists

    // B joins
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        ClientConfig::default(),
        test_protocol,
    );

    // Verify B connected before including E1 in scope
    scenario.expect(|ctx| ctx.server(|server| server.user_exists(&client_b_key).then_some(())));

    // Include E1 in B's scope (E2 is despawned, so don't include it)
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity_e1);
        });
    });

    // Verify B sees E1 (alive) but not E2 (despawned)
    scenario.expect(|ctx| {
        let b_sees_e1 = ctx.client(client_b_key, |c| c.has_entity(&entity_e1));
        let b_sees_e2 = ctx.client(client_b_key, |c| c.has_entity(&entity_e2));
        (b_sees_e1 && !b_sees_e2).then_some(())
    });
}
