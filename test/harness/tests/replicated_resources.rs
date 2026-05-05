//! End-to-end integration tests for Replicated Resources (R3 gate).
//!
//! Covers the R1 spec scenarios that are reachable in V1 (registration,
//! insert/remove dynamic+static, auto-scope on connect, late-join, and
//! re-insertion rejection). Authority delegation, priority, and Bevy
//! adapter scenarios are exercised in later phase gates.

use std::time::Duration;

use naia_client::{ClientConfig, JitterBufferType};
use naia_server::ServerConfig;
use naia_test_harness::{
    protocol, Auth, ClientConnectEvent, ClientKey, ServerAuthEvent, ServerConnectEvent, Scenario,
    TestMatchState, TestScore, ToTicks,
};

fn test_client_config() -> ClientConfig {
    let mut config = ClientConfig::default();
    config.send_handshake_interval = Duration::from_millis(0);
    config.jitter_buffer = JitterBufferType::Bypass;
    config
}

/// Bring up server, connect one client. Returns the connected client_key.
/// Mirrors the `client_connect` helper in legacy_tests/_helpers.rs but
/// inlined here so this file is self-contained.
fn server_with_one_client(scenario: &mut Scenario) -> ClientKey {
    let test_protocol = protocol();
    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));
    scenario.set_last_room(room_key);

    let client_auth = Auth::new("alice", "secret");
    let client_key = scenario.client_start(
        "alice",
        client_auth.clone(),
        test_client_config(),
        test_protocol,
    );

    // Server reads auth event
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((incoming_key, _)) = server.read_event::<ServerAuthEvent<Auth>>() {
                (incoming_key == client_key).then_some(())
            } else {
                None
            }
        })
    });

    // Accept
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.accept_connection(&client_key);
        });
    });

    // Wait for connect event + room membership
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some(_) = server.read_event::<ServerConnectEvent>() {
                Some(())
            } else {
                None
            }
        })
    });

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .room_mut(&room_key)
                .expect("room exists")
                .add_user(&client_key);
        });
    });

    scenario.expect(|ctx| {
        let connected = ctx.client(client_key, |c| c.connection_status().is_connected());
        let user_exists = ctx.server(|s| s.user_exists(&client_key));
        let _ = ctx.client(client_key, |c| c.read_event::<ClientConnectEvent>());
        (connected && user_exists).then_some(())
    });

    client_key
}

fn settle(scenario: &mut Scenario, ticks: usize) {
    for _ in 0..ticks {
        scenario.mutate(|_| {});
    }
}

#[test]
fn registration_sets_resource_kind_in_protocol() {
    let p = protocol();
    // TestScore was registered as a resource in test_protocol::protocol()
    let kind = naia_shared::ComponentKind::of::<TestScore>();
    assert!(
        p.resource_kinds.is_resource(&kind),
        "TestScore should be marked as a resource kind"
    );
    // Position is a regular component, not a resource.
    let pos_kind = naia_shared::ComponentKind::of::<naia_test_harness::Position>();
    assert!(
        !p.resource_kinds.is_resource(&pos_kind),
        "Position should NOT be a resource kind"
    );
}

#[test]
fn insert_dynamic_resource_replicates_to_connected_client() {
    let mut scenario = Scenario::new();
    let client_key = server_with_one_client(&mut scenario);

    // Server inserts a dynamic resource.
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            assert!(
                server.insert_resource(TestScore::new(7, 3)),
                "insert should succeed for fresh type"
            );
            assert!(server.has_resource::<TestScore>());
        });
    });

    // Allow replication round trip.
    settle(&mut scenario, 20);

    // Sanity: server-side value should be (7, 3).
    let val = scenario.mutate(|ctx| {
        ctx.server(|server| server.resource::<TestScore, _, _>(|s| (*s.home, *s.away)))
    });
    eprintln!("server.resource::<TestScore>() = {:?}", val);

    // Check client side count of entities
    let n = scenario.mutate(|ctx| ctx.client(client_key, |c| c.entities().len()));
    eprintln!("client entity count = {n}");

    // Client should see the resource via the harness scan.
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            c.resource::<TestScore, _, _>(|s| (*s.home, *s.away))
                .filter(|&(h, a)| h == 7 && a == 3)
                .map(|_| ())
        })
    });
}

#[test]
fn insert_static_resource_replicates_to_connected_client() {
    let mut scenario = Scenario::new();
    let client_key = server_with_one_client(&mut scenario);

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            assert!(server.insert_static_resource(TestMatchState::new(2)));
        });
    });

    settle(&mut scenario, 20);

    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            c.resource::<TestMatchState, _, _>(|s| *s.phase)
                .filter(|p| *p == 2)
                .map(|_| ())
        })
    });
}

#[test]
fn re_inserting_same_resource_returns_false() {
    let mut scenario = Scenario::new();
    let _client_key = server_with_one_client(&mut scenario);

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            assert!(server.insert_resource(TestScore::new(0, 0)));
            // Second insert returns false (ResourceAlreadyExists).
            assert!(!server.insert_resource(TestScore::new(99, 99)));
        });
    });

    settle(&mut scenario, 20);

    // Server-side value is the original (0, 0), not the rejected (99, 99).
    scenario.expect(|ctx| {
        ctx.server(|server| {
            server
                .resource::<TestScore, _, _>(|s| (*s.home, *s.away))
                .filter(|&v| v == (0, 0))
                .map(|_| ())
        })
    });
}

#[test]
fn remove_resource_propagates_to_client() {
    let mut scenario = Scenario::new();
    let client_key = server_with_one_client(&mut scenario);

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            assert!(server.insert_resource(TestScore::new(1, 2)));
        });
    });
    settle(&mut scenario, 20);

    // Client sees it
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| c.has_resource::<TestScore>().then_some(()))
    });

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            assert!(server.remove_resource::<TestScore>());
            assert!(!server.has_resource::<TestScore>());
        });
    });
    settle(&mut scenario, 20);

    // Client no longer sees it
    scenario.expect(|ctx| {
        ctx.client(
            client_key,
            |c| (!c.has_resource::<TestScore>()).then_some(()),
        )
    });
}

#[test]
fn server_mutation_replicates_to_client() {
    let mut scenario = Scenario::new();
    let client_key = server_with_one_client(&mut scenario);

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            assert!(server.insert_resource(TestScore::new(0, 0)));
        });
    });
    settle(&mut scenario, 20);

    // Initial state visible to client
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            c.resource::<TestScore, _, _>(|s| (*s.home, *s.away))
                .filter(|&v| v == (0, 0))
                .map(|_| ())
        })
    });

    // Mutate via the diff-tracked path.
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.mutate_resource::<TestScore, _, _>(|s| {
                *s.home = 42;
            });
        });
    });
    settle(&mut scenario, 20);

    // Client observes the update
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            c.resource::<TestScore, _, _>(|s| (*s.home, *s.away))
                .filter(|&v| v == (42, 0))
                .map(|_| ())
        })
    });

    // Multi-field mutation
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.mutate_resource::<TestScore, _, _>(|s| {
                *s.home = 1;
                *s.away = 2;
            });
        });
    });
    settle(&mut scenario, 20);
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            c.resource::<TestScore, _, _>(|s| (*s.home, *s.away))
                .filter(|&v| v == (1, 2))
                .map(|_| ())
        })
    });
}

#[test]
fn resource_priority_gain_is_settable() {
    let mut scenario = Scenario::new();
    let _client_key = server_with_one_client(&mut scenario);

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            assert!(server.insert_resource(TestScore::new(0, 0)));
            // Gain knob applies to a present resource.
            assert!(server.set_resource_priority_gain::<TestScore>(7.5));
            // Returns false for a not-inserted resource type.
            assert!(!server.set_resource_priority_gain::<TestMatchState>(2.0));
        });
    });
    settle(&mut scenario, 5);
}

#[test]
fn late_joining_client_observes_pre_existing_resource() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();
    scenario.server_start(ServerConfig::default(), test_protocol.clone());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));
    scenario.set_last_room(room_key);

    // Insert resource BEFORE any client connects.
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            assert!(server.insert_resource(TestScore::new(11, 22)));
        });
    });

    // Now connect a client.
    let client_auth = Auth::new("bob", "secret");
    let client_key = scenario.client_start(
        "bob",
        client_auth.clone(),
        test_client_config(),
        test_protocol,
    );

    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((k, _)) = server.read_event::<ServerAuthEvent<Auth>>() {
                (k == client_key).then_some(())
            } else {
                None
            }
        })
    });
    scenario.mutate(|ctx| ctx.server(|server| server.accept_connection(&client_key)));
    scenario.expect(|ctx| {
        ctx.server(|server| server.read_event::<ServerConnectEvent>().map(|_| ()))
    });
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .room_mut(&room_key)
                .expect("room exists")
                .add_user(&client_key);
        });
    });

    // Settle for the full handshake + initial replication round trip.
    settle(&mut scenario, 30);

    // Late-joining client should observe the resource.
    scenario.expect(|ctx| {
        ctx.client(client_key, |c| {
            c.resource::<TestScore, _, _>(|s| (*s.home, *s.away))
                .filter(|&v| v == (11, 22))
                .map(|_| ())
        })
    });
}
